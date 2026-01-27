use std::sync::Arc;

use crate::entry::{BrewEntry, BrewfileEntry, RestartService};
use crate::error::BrewfileError;
use crate::parser::Brewfile;

use zb_io::{Installer, ProgressCallback};
use zb_services::ServiceManager;

pub struct ImportPlan {
    pub to_install: Vec<BrewEntry>,
    pub already_installed: Vec<String>,
    pub unsupported: Vec<String>,
}

impl ImportPlan {
    pub fn total_to_install(&self) -> usize {
        self.to_install.len()
    }

    pub fn is_empty(&self) -> bool {
        self.to_install.is_empty()
    }
}

pub struct ImportResult {
    pub installed: Vec<String>,
    pub services_enabled: Vec<String>,
    pub failed: Vec<(String, String)>,
}

pub struct Importer<'a> {
    installer: &'a mut Installer,
    service_manager: Option<&'a mut ServiceManager>,
}

impl<'a> Importer<'a> {
    pub fn new(installer: &'a mut Installer) -> Self {
        Self {
            installer,
            service_manager: None,
        }
    }

    pub fn with_services(
        installer: &'a mut Installer,
        service_manager: &'a mut ServiceManager,
    ) -> Self {
        Self {
            installer,
            service_manager: Some(service_manager),
        }
    }

    pub fn plan(&self, brewfile: &Brewfile) -> ImportPlan {
        let brew_entries = brewfile.brew_entries();
        let unsupported = brewfile.unsupported_entries();

        let mut to_install = Vec::new();
        let mut already_installed = Vec::new();

        for entry in brew_entries {
            if self.installer.is_installed(&entry.name) {
                already_installed.push(entry.name.clone());
            } else {
                to_install.push(entry.clone());
            }
        }

        let unsupported_names: Vec<String> = unsupported
            .iter()
            .map(|e| format!("{} \"{}\"", e.entry_type(), Self::entry_name(e)))
            .collect();

        ImportPlan {
            to_install,
            already_installed,
            unsupported: unsupported_names,
        }
    }

    pub async fn execute(&mut self, plan: ImportPlan) -> Result<ImportResult, BrewfileError> {
        self.execute_with_progress(plan, None).await
    }

    pub async fn execute_with_progress(
        &mut self,
        plan: ImportPlan,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<ImportResult, BrewfileError> {
        let mut result = ImportResult {
            installed: Vec::new(),
            services_enabled: Vec::new(),
            failed: Vec::new(),
        };

        for entry in plan.to_install {
            // Install package
            match self.install_one(&entry, progress.clone()).await {
                Ok(_) => {
                    result.installed.push(entry.name.clone());

                    // Handle service hint
                    if let Some(restart_service) = entry.restart_service
                        && let Some(ref mut service_manager) = self.service_manager
                    {
                        match restart_service {
                            RestartService::Always => {
                                // Enable and start service
                                if let Err(e) = service_manager.enable(&entry.name).await {
                                    eprintln!(
                                        "Warning: Failed to enable service {}: {}",
                                        entry.name, e
                                    );
                                } else {
                                    result.services_enabled.push(entry.name.clone());
                                }
                            }
                            RestartService::Changed => {
                                // For initial install, Changed means enable
                                // (Changed vs Always differs on upgrades, not initial installs)
                                if let Err(e) = service_manager.enable(&entry.name).await {
                                    eprintln!(
                                        "Warning: Failed to enable service {}: {}",
                                        entry.name, e
                                    );
                                } else {
                                    result.services_enabled.push(entry.name.clone());
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    result.failed.push((entry.name.clone(), e.to_string()));
                }
            }
        }

        Ok(result)
    }

    async fn install_one(
        &mut self,
        entry: &BrewEntry,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<(), BrewfileError> {
        let link = entry.link.unwrap_or(true);

        let install_plan = self
            .installer
            .plan(&entry.name)
            .await
            .map_err(|e| BrewfileError::InstallError(e.to_string()))?;

        self.installer
            .execute_with_progress(install_plan, link, progress)
            .await
            .map_err(|e| BrewfileError::InstallError(e.to_string()))?;

        Ok(())
    }

    fn entry_name(entry: &BrewfileEntry) -> &str {
        match entry {
            BrewfileEntry::Tap { name, .. } => name,
            BrewfileEntry::Brew(brew) => &brew.name,
            BrewfileEntry::Cask { name } => name,
            BrewfileEntry::Mas { name, .. } => name,
            BrewfileEntry::Vscode { name } => name,
            BrewfileEntry::Go { name } => name,
            BrewfileEntry::Cargo { name } => name,
            BrewfileEntry::Flatpak { name, .. } => name,
        }
    }
}

#[cfg(test)]
mod tests {

    // Integration tests would require mock installer
    // Basic plan logic can be tested here
}
