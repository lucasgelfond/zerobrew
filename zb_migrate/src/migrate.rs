use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use crate::scanner::{HomebrewScanner, InstalledFormula, ScanError};

use zb_io::ProgressCallback;
use zb_io::install::Installer;

/// Reason why a formula cannot be migrated
#[derive(Debug, Clone)]
pub enum IncompatibleReason {
    /// Requires a custom tap that Zerobrew can't access
    RequiresTap(String),
    /// Already installed in Zerobrew
    AlreadyInstalled,
    /// API error when checking formula
    ApiError(String),
}

/// A formula that cannot be migrated
#[derive(Debug, Clone)]
pub struct Incompatible {
    pub name: String,
    pub reason: IncompatibleReason,
}

/// Migration plan showing what will be installed
#[derive(Debug, Clone)]
pub struct MigrationPlan {
    /// Packages explicitly requested by user (will be installed)
    pub to_install: Vec<String>,
    /// Packages that are dependencies (will be auto-installed)
    pub dependencies: Vec<String>,
    /// Packages that cannot be migrated
    pub incompatible: Vec<Incompatible>,
    /// Packages already installed in Zerobrew
    pub already_installed: Vec<String>,
    /// Packages with running services (informational warning)
    pub services_warning: Vec<String>,
}

impl MigrationPlan {
    /// Check if there's anything to migrate
    pub fn is_empty(&self) -> bool {
        self.to_install.is_empty()
    }

    /// Total packages that will be installed (requested + deps)
    pub fn total_to_install(&self) -> usize {
        self.to_install.len()
    }
}

/// Result of migration execution
#[derive(Debug)]
pub struct MigrationResult {
    /// Successfully installed packages
    pub installed: Vec<String>,
    /// Failed packages
    pub failed: Vec<(String, String)>,
}

/// Migrates packages from Homebrew to Zerobrew
pub struct Migrator<'a> {
    scanner: HomebrewScanner,
    installer: &'a mut Installer,
}

impl<'a> Migrator<'a> {
    /// Create a new migrator with default Homebrew prefix
    pub fn new(installer: &'a mut Installer) -> Self {
        Self {
            scanner: HomebrewScanner::new(),
            installer,
        }
    }

    /// Create a migrator with custom Homebrew prefix
    pub fn with_prefix(installer: &'a mut Installer, homebrew_prefix: &Path) -> Self {
        Self {
            scanner: HomebrewScanner::with_prefix(homebrew_prefix.to_path_buf()),
            installer,
        }
    }

    /// Check if Homebrew is installed
    pub fn is_homebrew_installed(&self) -> bool {
        self.scanner.is_homebrew_installed()
    }

    /// Scan Homebrew and create a migration plan
    pub fn plan(
        &self,
        specific_formulas: Option<&[String]>,
    ) -> Result<MigrationPlan, MigrationError> {
        // Scan Homebrew
        let all_formulas = self.scanner.scan()?;

        if all_formulas.is_empty() {
            return Ok(MigrationPlan {
                to_install: Vec::new(),
                dependencies: Vec::new(),
                incompatible: Vec::new(),
                already_installed: Vec::new(),
                services_warning: Vec::new(),
            });
        }

        // Determine which formulas to migrate
        let to_migrate: Vec<&InstalledFormula> = if let Some(names) = specific_formulas {
            let name_set: HashSet<&str> = names.iter().map(|s| s.as_str()).collect();
            all_formulas
                .iter()
                .filter(|f| name_set.contains(f.name.as_str()))
                .collect()
        } else {
            // Migrate all user-requested formulas
            all_formulas
                .iter()
                .filter(|f| f.installed_on_request || !f.installed_as_dependency)
                .collect()
        };

        let mut plan = MigrationPlan {
            to_install: Vec::new(),
            dependencies: Vec::new(),
            incompatible: Vec::new(),
            already_installed: Vec::new(),
            services_warning: Vec::new(),
        };

        // Categorize each formula
        for formula in to_migrate {
            // Check if already installed in Zerobrew
            if self.installer.is_installed(&formula.name) {
                plan.already_installed.push(formula.name.clone());
                continue;
            }

            // Check if it's from a custom tap
            if let Some(tap) = &formula.tap
                && tap != "homebrew/core"
                && !tap.is_empty()
            {
                plan.incompatible.push(Incompatible {
                    name: formula.name.clone(),
                    reason: IncompatibleReason::RequiresTap(tap.clone()),
                });
                continue;
            }

            plan.to_install.push(formula.name.clone());
        }

        // Collect dependencies (for informational purposes)
        let to_install_set: HashSet<&str> = plan.to_install.iter().map(|s| s.as_str()).collect();
        for formula in &all_formulas {
            if formula.installed_as_dependency
                && !to_install_set.contains(formula.name.as_str())
                && !plan.already_installed.contains(&formula.name)
            {
                plan.dependencies.push(formula.name.clone());
            }
        }

        Ok(plan)
    }

    /// Execute the migration plan
    pub async fn execute(
        &mut self,
        plan: &MigrationPlan,
    ) -> Result<MigrationResult, MigrationError> {
        self.execute_with_progress(plan, None).await
    }

    /// Execute the migration plan with progress callback
    pub async fn execute_with_progress(
        &mut self,
        plan: &MigrationPlan,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<MigrationResult, MigrationError> {
        let mut result = MigrationResult {
            installed: Vec::new(),
            failed: Vec::new(),
        };

        for name in &plan.to_install {
            match self.install_one(name, progress.clone()).await {
                Ok(_) => {
                    result.installed.push(name.clone());
                }
                Err(e) => {
                    result.failed.push((name.clone(), e.to_string()));
                }
            }
        }

        Ok(result)
    }

    async fn install_one(
        &mut self,
        name: &str,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<(), MigrationError> {
        // Use the installer's plan and execute methods
        let install_plan = self
            .installer
            .plan(name)
            .await
            .map_err(|e| MigrationError::InstallError(name.to_string(), e.to_string()))?;

        self.installer
            .execute_with_progress(install_plan, true, progress)
            .await
            .map_err(|e| MigrationError::InstallError(name.to_string(), e.to_string()))?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("scan error: {0}")]
    ScanError(#[from] ScanError),

    #[error("failed to install {0}: {1}")]
    InstallError(String, String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_plan_is_empty_when_no_installs() {
        let plan = MigrationPlan {
            to_install: vec![],
            dependencies: vec![],
            incompatible: vec![],
            already_installed: vec![],
            services_warning: vec![],
        };

        assert!(plan.is_empty());
        assert_eq!(plan.total_to_install(), 0);
    }

    #[test]
    fn migration_plan_not_empty_with_packages() {
        let plan = MigrationPlan {
            to_install: vec!["wget".to_string(), "jq".to_string()],
            dependencies: vec!["openssl".to_string()],
            incompatible: vec![],
            already_installed: vec![],
            services_warning: vec![],
        };

        assert!(!plan.is_empty());
        assert_eq!(plan.total_to_install(), 2);
    }

    #[test]
    fn incompatible_reason_requires_tap() {
        let incomp = Incompatible {
            name: "custom-formula".to_string(),
            reason: IncompatibleReason::RequiresTap("user/custom".to_string()),
        };

        assert_eq!(incomp.name, "custom-formula");
        match incomp.reason {
            IncompatibleReason::RequiresTap(tap) => assert_eq!(tap, "user/custom"),
            _ => panic!("Expected RequiresTap"),
        }
    }

    #[test]
    fn incompatible_reason_already_installed() {
        let incomp = Incompatible {
            name: "wget".to_string(),
            reason: IncompatibleReason::AlreadyInstalled,
        };

        match incomp.reason {
            IncompatibleReason::AlreadyInstalled => {}
            _ => panic!("Expected AlreadyInstalled"),
        }
    }

    #[test]
    fn incompatible_reason_api_error() {
        let incomp = Incompatible {
            name: "broken-pkg".to_string(),
            reason: IncompatibleReason::ApiError("404 Not Found".to_string()),
        };

        match incomp.reason {
            IncompatibleReason::ApiError(err) => assert_eq!(err, "404 Not Found"),
            _ => panic!("Expected ApiError"),
        }
    }

    #[test]
    fn migration_result_tracks_success_and_failure() {
        let result = MigrationResult {
            installed: vec!["wget".to_string(), "jq".to_string()],
            failed: vec![("curl".to_string(), "Network error".to_string())],
        };

        assert_eq!(result.installed.len(), 2);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].0, "curl");
    }
}
