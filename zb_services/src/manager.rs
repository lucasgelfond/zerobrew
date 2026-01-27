use std::path::{Path, PathBuf};

use crate::definition::ServiceDefinition;
use crate::error::ServiceError;
use crate::launchd::{LaunchdManager, ServiceState};
use crate::validation::validate_service;

use zb_io::{ApiClient, Database};

#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub name: String,
    pub plist_name: String,
    pub state: ServiceState,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub loaded_file: Option<PathBuf>,
    pub enabled: bool,
}

pub struct ServiceManager {
    launchd: LaunchdManager,
    api_client: ApiClient,
    db: Database,
    prefix: PathBuf,
}

impl ServiceManager {
    pub fn new(prefix: &Path, db: Database) -> Result<Self, ServiceError> {
        Ok(Self {
            launchd: LaunchdManager::new()?,
            api_client: ApiClient::new(),
            db,
            prefix: prefix.to_path_buf(),
        })
    }

    pub async fn start(&mut self, formula: &str) -> Result<(), ServiceError> {
        // 1. Get raw formula JSON from API
        let formula_data = self
            .api_client
            .get_formula_raw(formula)
            .await
            .map_err(ServiceError::Database)?;

        // 2. Extract service definition from formula JSON
        let service_json = formula_data
            .get("service")
            .cloned()
            .ok_or_else(|| ServiceError::NoServiceDefinition(formula.to_string()))?;

        let mut service_def: ServiceDefinition = serde_json::from_value(service_json)?;
        service_def.formula_name = formula.to_string();

        // Set plist_name if not provided
        if service_def.plist_name.is_empty() {
            service_def.plist_name = format!("homebrew.mxcl.{}", formula);
        }

        // 3. Validate service definition
        validate_service(&service_def)?;

        // 4. Generate plist
        let plist_content =
            crate::launchd::plist::PlistGenerator::generate(&service_def, &self.prefix)?;

        // 5. Write plist to LaunchAgents
        let plist_path = self.get_plist_path(&service_def)?;
        self.write_plist_securely(&plist_path, &plist_content)?;

        // 6. Bootstrap with launchd
        self.launchd.bootstrap(&plist_path)?;

        // 7. Kickstart the service
        self.launchd.kickstart(&service_def.plist_name)?;

        Ok(())
    }

    pub fn stop(&mut self, formula: &str) -> Result<(), ServiceError> {
        // Construct plist name
        let plist_name = format!("homebrew.mxcl.{}", formula);

        // Bootout from launchd
        self.launchd.bootout(&plist_name)?;

        Ok(())
    }

    pub async fn restart(&mut self, formula: &str) -> Result<(), ServiceError> {
        self.stop(formula)?;
        self.start(formula).await?;
        Ok(())
    }

    pub fn status(&self, formula: &str) -> Result<ServiceStatus, ServiceError> {
        let plist_name = format!("homebrew.mxcl.{}", formula);

        let status = self.launchd.print_status(&plist_name)?;
        let enabled = self
            .db
            .is_service_enabled(formula)
            .map_err(ServiceError::Database)?;

        Ok(ServiceStatus {
            name: formula.to_string(),
            plist_name,
            state: status.state,
            pid: status.pid,
            exit_code: status.exit_code,
            loaded_file: status.loaded_file,
            enabled,
        })
    }

    pub fn list(&self) -> Result<Vec<ServiceStatus>, ServiceError> {
        let launch_agents = Self::user_launch_agents_dir();

        if !launch_agents.exists() {
            return Ok(Vec::new());
        }

        let mut services = Vec::new();

        // Scan LaunchAgents for homebrew.mxcl.*.plist files
        let entries = std::fs::read_dir(&launch_agents)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let filename = path.file_name().and_then(|n| n.to_str());

            if let Some(name) = filename
                && name.starts_with("homebrew.mxcl.")
                && name.ends_with(".plist")
            {
                // Extract formula name from plist name
                let formula_name = name
                    .trim_start_matches("homebrew.mxcl.")
                    .trim_end_matches(".plist");

                let status = self.status(formula_name)?;
                services.push(status);
            }
        }

        Ok(services)
    }

    pub async fn enable(&mut self, formula: &str) -> Result<(), ServiceError> {
        // Mark as enabled in database
        self.db
            .set_service_enabled(formula, true)
            .map_err(ServiceError::Database)?;

        // If not currently loaded, start it
        let plist_name = format!("homebrew.mxcl.{}", formula);
        let status = self.launchd.print_status(&plist_name)?;

        if status.state != ServiceState::Running {
            self.start(formula).await?;
        }

        Ok(())
    }

    pub fn disable(&mut self, formula: &str) -> Result<(), ServiceError> {
        // Mark as disabled in database
        self.db
            .set_service_enabled(formula, false)
            .map_err(ServiceError::Database)?;

        // Note: Does NOT stop the service, just prevents auto-start
        Ok(())
    }

    fn get_plist_path(&self, def: &ServiceDefinition) -> Result<PathBuf, ServiceError> {
        let launch_agents = Self::user_launch_agents_dir();

        // Create directory if it doesn't exist
        std::fs::create_dir_all(&launch_agents)?;

        Ok(launch_agents.join(format!("{}.plist", def.plist_name)))
    }

    fn user_launch_agents_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join("Library/LaunchAgents")
    }

    fn write_plist_securely(&self, path: &Path, content: &str) -> Result<(), ServiceError> {
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        // Validate destination path
        self.validate_plist_path(path)?;

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write with proper permissions (rw-r--r--)
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(path)?;

        file.write_all(content.as_bytes())?;
        file.sync_all()?;

        // Verify ownership
        self.verify_plist_ownership(path)?;

        Ok(())
    }

    fn validate_plist_path(&self, path: &Path) -> Result<(), ServiceError> {
        // Must be in ~/Library/LaunchAgents
        let launch_agents = Self::user_launch_agents_dir();

        if !path.starts_with(&launch_agents) {
            return Err(ServiceError::InvalidPlistLocation(path.to_path_buf()));
        }

        // Check that parent directory isn't a symlink (symlink attack protection)
        if let Some(parent) = path.parent()
            && parent.exists()
        {
            let metadata = std::fs::symlink_metadata(parent)?;
            if metadata.is_symlink() {
                return Err(ServiceError::SymlinkAttack(parent.to_path_buf()));
            }
        }

        Ok(())
    }

    fn verify_plist_ownership(&self, path: &Path) -> Result<(), ServiceError> {
        use std::os::unix::fs::MetadataExt;

        let metadata = std::fs::metadata(path)?;
        let file_uid = metadata.uid();
        let current_uid = unsafe { libc::getuid() };

        if file_uid != current_uid {
            return Err(ServiceError::PermissionDenied(format!(
                "plist file {} not owned by current user",
                path.display()
            )));
        }

        Ok(())
    }
}
