use std::path::PathBuf;

use crate::tab::{HomebrewTab, TabError};

/// Information about an installed Homebrew formula
#[derive(Debug, Clone)]
pub struct InstalledFormula {
    pub name: String,
    pub version: String,
    pub full_name: String,
    pub installed_on_request: bool,
    pub installed_as_dependency: bool,
    pub poured_from_bottle: bool,
    pub tap: Option<String>,
    pub runtime_dependencies: Vec<String>,
    pub keg_path: PathBuf,
}

/// Scanner for discovering Homebrew installations
pub struct HomebrewScanner {
    homebrew_prefix: PathBuf,
}

impl HomebrewScanner {
    /// Create a scanner for the default Homebrew prefix
    pub fn new() -> Self {
        Self::with_prefix(PathBuf::from("/opt/homebrew"))
    }

    /// Create a scanner for a custom prefix
    pub fn with_prefix(prefix: PathBuf) -> Self {
        Self {
            homebrew_prefix: prefix,
        }
    }

    /// Get the Cellar path
    pub fn cellar_path(&self) -> PathBuf {
        self.homebrew_prefix.join("Cellar")
    }

    /// Check if Homebrew is installed at this prefix
    pub fn is_homebrew_installed(&self) -> bool {
        self.cellar_path().exists()
    }

    /// Scan and return all installed formulas
    pub fn scan(&self) -> Result<Vec<InstalledFormula>, ScanError> {
        let cellar = self.cellar_path();

        if !cellar.exists() {
            return Err(ScanError::NoCellar(cellar));
        }

        let mut formulas = Vec::new();

        // Iterate over formula directories in Cellar
        let entries = std::fs::read_dir(&cellar)
            .map_err(|e| ScanError::ReadError(cellar.clone(), e.to_string()))?;

        for entry in entries {
            let entry = entry.map_err(|e| ScanError::ReadError(cellar.clone(), e.to_string()))?;
            let formula_path = entry.path();

            if !formula_path.is_dir() {
                continue;
            }

            let name = match formula_path.file_name() {
                Some(n) => n.to_string_lossy().to_string(),
                None => continue,
            };

            // Find version directories
            if let Ok(versions) = std::fs::read_dir(&formula_path) {
                for version_entry in versions.flatten() {
                    let version_path = version_entry.path();
                    if !version_path.is_dir() {
                        continue;
                    }

                    let version = match version_path.file_name() {
                        Some(v) => v.to_string_lossy().to_string(),
                        None => continue,
                    };

                    // Try to read the Tab file
                    let tab_path = version_path.join("INSTALL_RECEIPT.json");
                    let tab = if tab_path.exists() {
                        HomebrewTab::from_file(&tab_path).ok()
                    } else {
                        None
                    };

                    let formula = InstalledFormula {
                        name: name.clone(),
                        version: version.clone(),
                        full_name: name.clone(), // Will be updated with tap info if available
                        installed_on_request: tab
                            .as_ref()
                            .map(|t| t.installed_on_request)
                            .unwrap_or(false),
                        installed_as_dependency: tab
                            .as_ref()
                            .map(|t| t.installed_as_dependency)
                            .unwrap_or(false),
                        poured_from_bottle: tab
                            .as_ref()
                            .map(|t| t.poured_from_bottle)
                            .unwrap_or(false),
                        tap: tab.as_ref().and_then(|t| t.tap().map(String::from)),
                        runtime_dependencies: tab
                            .as_ref()
                            .and_then(|t| t.runtime_dependencies.as_ref())
                            .map(|deps| deps.iter().map(|d| d.full_name.clone()).collect())
                            .unwrap_or_default(),
                        keg_path: version_path,
                    };

                    formulas.push(formula);
                }
            }
        }

        Ok(formulas)
    }

    /// Scan and return only user-requested formulas (not installed as dependencies)
    pub fn scan_requested(&self) -> Result<Vec<InstalledFormula>, ScanError> {
        let all = self.scan()?;
        Ok(all
            .into_iter()
            .filter(|f| f.installed_on_request || !f.installed_as_dependency)
            .collect())
    }

    /// Check if a specific formula is installed
    pub fn is_installed(&self, name: &str) -> bool {
        let formula_path = self.cellar_path().join(name);
        formula_path.exists() && formula_path.is_dir()
    }

    /// Get the installed version of a formula
    pub fn get_installed_version(&self, name: &str) -> Option<String> {
        let formula_path = self.cellar_path().join(name);
        if !formula_path.exists() {
            return None;
        }

        // Find the latest version directory
        std::fs::read_dir(&formula_path)
            .ok()?
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().to_str().map(String::from))
            .max() // Simple lexicographic comparison for now
    }
}

impl Default for HomebrewScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("Homebrew Cellar not found at {0}")]
    NoCellar(PathBuf),

    #[error("failed to read {0}: {1}")]
    ReadError(PathBuf, String),

    #[error("failed to parse tab: {0}")]
    TabError(#[from] TabError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_cellar() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let cellar = tmp.path().join("Cellar");

        // Create a fake jq installation
        let jq_path = cellar.join("jq").join("1.7.1");
        fs::create_dir_all(&jq_path).unwrap();

        let tab = r#"{
            "homebrew_version": "4.0.0",
            "installed_on_request": true,
            "installed_as_dependency": false,
            "poured_from_bottle": true,
            "source": {"tap": "homebrew/core"}
        }"#;
        fs::write(jq_path.join("INSTALL_RECEIPT.json"), tab).unwrap();

        // Create a fake openssl (dependency)
        let openssl_path = cellar.join("openssl@3").join("3.2.0");
        fs::create_dir_all(&openssl_path).unwrap();

        let tab_dep = r#"{
            "homebrew_version": "4.0.0",
            "installed_on_request": false,
            "installed_as_dependency": true,
            "poured_from_bottle": true
        }"#;
        fs::write(openssl_path.join("INSTALL_RECEIPT.json"), tab_dep).unwrap();

        tmp
    }

    #[test]
    fn scan_finds_formulas() {
        let tmp = create_test_cellar();
        let scanner = HomebrewScanner::with_prefix(tmp.path().to_path_buf());

        let formulas = scanner.scan().unwrap();
        assert_eq!(formulas.len(), 2);

        let jq = formulas.iter().find(|f| f.name == "jq").unwrap();
        assert_eq!(jq.version, "1.7.1");
        assert!(jq.installed_on_request);
        assert!(!jq.installed_as_dependency);
    }

    #[test]
    fn scan_requested_filters_dependencies() {
        let tmp = create_test_cellar();
        let scanner = HomebrewScanner::with_prefix(tmp.path().to_path_buf());

        let requested = scanner.scan_requested().unwrap();
        assert_eq!(requested.len(), 1);
        assert_eq!(requested[0].name, "jq");
    }

    #[test]
    fn missing_cellar_returns_error() {
        let scanner = HomebrewScanner::with_prefix(PathBuf::from("/nonexistent/path"));
        assert!(scanner.scan().is_err());
    }
}
