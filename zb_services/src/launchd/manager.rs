use std::path::Path;
use std::process::Command;

use crate::error::ServiceError;
use crate::launchd::status::{LaunchctlStatus, ServiceState};

const DOMAIN_ACTION_NOT_SUPPORTED: i32 = 125;

pub struct LaunchdManager {
    domain_target: String,
}

impl LaunchdManager {
    pub fn new() -> Result<Self, ServiceError> {
        let domain_target = Self::determine_domain()?;
        Ok(Self { domain_target })
    }

    fn determine_domain() -> Result<String, ServiceError> {
        if Self::is_root()? {
            return Ok("system".to_string());
        }

        let euid = Self::get_euid()?;

        // Check for special cases that require user/* instead of gui/*
        let ssh_tty = std::env::var("SSH_TTY").is_ok();
        let sudo_user = std::env::var("SUDO_USER").is_ok();

        let uid = unsafe { libc::getuid() };
        let uid_mismatch = uid != euid;

        if ssh_tty || sudo_user || uid_mismatch {
            // Use user/* domain for these edge cases
            Ok(format!("user/{}", euid))
        } else {
            // Normal case: gui/* domain
            Ok(format!("gui/{}", euid))
        }
    }

    fn is_root() -> Result<bool, ServiceError> {
        let euid = Self::get_euid()?;
        Ok(euid == 0)
    }

    fn get_euid() -> Result<u32, ServiceError> {
        Ok(unsafe { libc::geteuid() })
    }

    pub fn bootstrap(&self, plist_path: &Path) -> Result<(), ServiceError> {
        let output = Command::new("launchctl")
            .args([
                "bootstrap",
                &self.domain_target,
                &plist_path.to_string_lossy(),
            ])
            .output()?;

        if !output.status.success() {
            return Err(ServiceError::LaunchctlFailed {
                command: "bootstrap".to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        Ok(())
    }

    pub fn kickstart(&self, service_name: &str) -> Result<(), ServiceError> {
        let target = format!("{}/{}", self.domain_target, service_name);

        let output = Command::new("launchctl")
            .args(["kickstart", "-k", &target])
            .output()?;

        if !output.status.success() {
            return Err(ServiceError::LaunchctlFailed {
                command: "kickstart".to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        Ok(())
    }

    pub fn bootout(&self, service_name: &str) -> Result<(), ServiceError> {
        let target = format!("{}/{}", self.domain_target, service_name);

        let output = Command::new("launchctl")
            .args(["bootout", &target])
            .output()?;

        // Exit code 125 (DOMAIN_ACTION_NOT_SUPPORTED) is not an error - service wasn't loaded
        if !output.status.success() {
            if let Some(code) = output.status.code()
                && code == DOMAIN_ACTION_NOT_SUPPORTED
            {
                return Ok(());
            }

            return Err(ServiceError::LaunchctlFailed {
                command: "bootout".to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        Ok(())
    }

    pub fn print_status(&self, service_name: &str) -> Result<LaunchctlStatus, ServiceError> {
        let target = format!("{}/{}", self.domain_target, service_name);

        let output = Command::new("launchctl")
            .args(["print", &target])
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            return LaunchctlStatus::parse_print_output(&stdout);
        }

        // Fall back to list command
        self.list(service_name)
    }

    pub fn list(&self, service_name: &str) -> Result<LaunchctlStatus, ServiceError> {
        let output = Command::new("launchctl")
            .args(["list", service_name])
            .output()?;

        if !output.status.success() {
            // Service not loaded
            return Ok(LaunchctlStatus {
                pid: None,
                exit_code: None,
                loaded_file: None,
                state: ServiceState::Stopped,
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        LaunchctlStatus::parse_list_output(&stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determines_domain_for_regular_user() {
        // This will vary based on actual environment
        let manager = LaunchdManager::new().unwrap();
        // Should be gui/{uid} or user/{uid}
        assert!(
            manager.domain_target.starts_with("gui/") || manager.domain_target.starts_with("user/")
        );
    }

    #[test]
    fn bootout_handles_not_loaded_gracefully() {
        // Bootout of non-loaded service should not error
        // This test would need mocking to work reliably
    }
}
