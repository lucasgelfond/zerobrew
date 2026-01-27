use serde::Deserialize;
use std::path::Path;

/// Represents a runtime dependency from Homebrew's Tab
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeDep {
    pub full_name: String,
    pub version: String,
    #[serde(default)]
    pub pkg_version: String,
}

/// Source information from Homebrew's Tab
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TabSource {
    pub tap: Option<String>,
    pub spec: Option<String>,
}

/// Represents Homebrew's INSTALL_RECEIPT.json (Tab) file
#[derive(Debug, Clone, Deserialize)]
pub struct HomebrewTab {
    #[serde(default)]
    pub homebrew_version: String,

    #[serde(default)]
    pub installed_as_dependency: bool,

    #[serde(default)]
    pub installed_on_request: bool,

    #[serde(default)]
    pub poured_from_bottle: bool,

    #[serde(default)]
    pub runtime_dependencies: Option<Vec<RuntimeDep>>,

    #[serde(default)]
    pub time: Option<i64>,

    #[serde(default)]
    pub arch: Option<String>,

    #[serde(default)]
    pub source: TabSource,
}

impl HomebrewTab {
    /// Parse a Tab from an INSTALL_RECEIPT.json file
    pub fn from_file(path: &Path) -> Result<Self, TabError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| TabError::ReadError(path.to_path_buf(), e.to_string()))?;

        Self::parse(&content)
    }

    /// Parse a Tab from JSON string
    pub fn parse(content: &str) -> Result<Self, TabError> {
        serde_json::from_str(content).map_err(|e| TabError::ParseError(e.to_string()))
    }

    /// Get the tap name (e.g., "homebrew/core")
    pub fn tap(&self) -> Option<&str> {
        self.source.tap.as_deref()
    }

    /// Check if this was installed from homebrew/core (or no tap, which implies core)
    pub fn is_core_formula(&self) -> bool {
        match self.tap() {
            None => true,
            Some(tap) => tap == "homebrew/core",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TabError {
    #[error("failed to read tab file {0}: {1}")]
    ReadError(std::path::PathBuf, String),

    #[error("failed to parse tab JSON: {0}")]
    ParseError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_tab() {
        let json = r#"{
            "homebrew_version": "4.0.0",
            "installed_on_request": true,
            "poured_from_bottle": true
        }"#;

        let tab = HomebrewTab::parse(json).unwrap();
        assert_eq!(tab.homebrew_version, "4.0.0");
        assert!(tab.installed_on_request);
        assert!(tab.poured_from_bottle);
        assert!(!tab.installed_as_dependency);
    }

    #[test]
    fn parse_tab_with_dependencies() {
        let json = r#"{
            "homebrew_version": "4.0.0",
            "installed_on_request": true,
            "runtime_dependencies": [
                {"full_name": "openssl@3", "version": "3.2.0", "pkg_version": "3.2.0"},
                {"full_name": "readline", "version": "8.2", "pkg_version": "8.2.1"}
            ],
            "source": {
                "tap": "homebrew/core",
                "spec": "stable"
            }
        }"#;

        let tab = HomebrewTab::parse(json).unwrap();
        // Check tap first before consuming runtime_dependencies
        assert!(tab.is_core_formula());

        let deps = tab.runtime_dependencies.as_ref().unwrap();
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].full_name, "openssl@3");
    }

    #[test]
    fn parse_tab_with_custom_tap() {
        let json = r#"{
            "homebrew_version": "4.0.0",
            "source": {
                "tap": "user/custom-tap"
            }
        }"#;

        let tab = HomebrewTab::parse(json).unwrap();
        assert_eq!(tab.tap(), Some("user/custom-tap"));
        assert!(!tab.is_core_formula());
    }
}
