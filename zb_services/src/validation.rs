use std::path::Path;

use crate::definition::ServiceDefinition;
use crate::error::ServiceError;

/// Validate a service definition for security and correctness
pub fn validate_service(def: &ServiceDefinition) -> Result<(), ServiceError> {
    // Validate nice values
    if let Some(nice) = def.nice {
        if !(-20..=19).contains(&nice) {
            return Err(ServiceError::InvalidDefinition(format!(
                "nice value {} out of range (-20 to 19)",
                nice
            )));
        }

        // Negative nice requires root
        if nice < 0 && !def.require_root {
            return Err(ServiceError::InvalidDefinition(
                "negative nice value requires require_root: true".to_string(),
            ));
        }
    }

    // Validate run command exists
    let command = match &def.run {
        crate::definition::RunCommand::Simple(cmd) => cmd.clone(),
        crate::definition::RunCommand::PlatformSpecific { macos, .. } => {
            macos.clone().ok_or_else(|| {
                ServiceError::InvalidDefinition("no macOS run command specified".to_string())
            })?
        }
    };

    if command.is_empty() {
        return Err(ServiceError::InvalidDefinition(
            "run command cannot be empty".to_string(),
        ));
    }

    // Validate paths if provided
    if let Some(ref wd) = def.working_dir {
        validate_path(wd, "working_dir")?;
    }

    if let Some(ref rd) = def.root_dir {
        validate_path(rd, "root_dir")?;
    }

    if let Some(ref lp) = def.log_path {
        validate_path(lp, "log_path")?;
    }

    if let Some(ref elp) = def.error_log_path {
        validate_path(elp, "error_log_path")?;
    }

    // Validate keep_alive path
    if let Some(ref path) = def.keep_alive.path {
        validate_path(path, "keep_alive.path")?;
    }

    Ok(())
}

pub fn validate_path(path: &str, field: &str) -> Result<(), ServiceError> {
    // Check for path traversal
    if path.contains("..") {
        return Err(ServiceError::PathTraversal(format!(
            "{}: path contains '..'",
            field
        )));
    }

    // Disallow absolute paths outside safe directories
    if path.starts_with('/') {
        let allowed_prefixes = [
            "/opt/zerobrew/",
            "/opt/homebrew/",
            "/usr/local/",
            "/tmp/",
            "/var/",
        ];

        if !allowed_prefixes
            .iter()
            .any(|prefix| path.starts_with(prefix))
        {
            return Err(ServiceError::PathTraversal(format!(
                "{}: absolute path '{}' outside allowed directories",
                field, path
            )));
        }
    }

    Ok(())
}

pub fn validate_executable(path: &Path) -> Result<(), ServiceError> {
    // Check if file exists
    if !path.exists() {
        return Err(ServiceError::ExecutableNotFound(path.to_path_buf()));
    }

    // Check if executable
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::metadata(path)?;
    let permissions = metadata.permissions();
    let mode = permissions.mode();

    // Check if any execute bit is set (owner, group, or other)
    if mode & 0o111 == 0 {
        return Err(ServiceError::NotExecutable(path.to_path_buf()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::RunCommand;

    #[test]
    fn rejects_out_of_range_nice() {
        let def = ServiceDefinition {
            formula_name: "test".to_string(),
            plist_name: "homebrew.mxcl.test".to_string(),
            run: RunCommand::Simple(vec!["/bin/test".to_string()]),
            nice: Some(100),
            ..Default::default()
        };

        assert!(validate_service(&def).is_err());
    }

    #[test]
    fn rejects_negative_nice_without_root() {
        let def = ServiceDefinition {
            formula_name: "test".to_string(),
            plist_name: "homebrew.mxcl.test".to_string(),
            run: RunCommand::Simple(vec!["/bin/test".to_string()]),
            nice: Some(-5),
            require_root: false,
            ..Default::default()
        };

        assert!(validate_service(&def).is_err());
    }

    #[test]
    fn accepts_negative_nice_with_root() {
        let def = ServiceDefinition {
            formula_name: "test".to_string(),
            plist_name: "homebrew.mxcl.test".to_string(),
            run: RunCommand::Simple(vec!["/bin/test".to_string()]),
            nice: Some(-5),
            require_root: true,
            ..Default::default()
        };

        assert!(validate_service(&def).is_ok());
    }

    #[test]
    fn rejects_path_traversal_in_working_dir() {
        let def = ServiceDefinition {
            formula_name: "test".to_string(),
            plist_name: "homebrew.mxcl.test".to_string(),
            run: RunCommand::Simple(vec!["/bin/test".to_string()]),
            working_dir: Some("../../etc".to_string()),
            ..Default::default()
        };

        assert!(validate_service(&def).is_err());
    }

    #[test]
    fn rejects_disallowed_absolute_path() {
        let def = ServiceDefinition {
            formula_name: "test".to_string(),
            plist_name: "homebrew.mxcl.test".to_string(),
            run: RunCommand::Simple(vec!["/bin/test".to_string()]),
            log_path: Some("/etc/passwd".to_string()),
            ..Default::default()
        };

        assert!(validate_service(&def).is_err());
    }

    #[test]
    fn accepts_valid_paths() {
        let def = ServiceDefinition {
            formula_name: "test".to_string(),
            plist_name: "homebrew.mxcl.test".to_string(),
            run: RunCommand::Simple(vec!["/bin/test".to_string()]),
            working_dir: Some("/opt/zerobrew/prefix".to_string()),
            log_path: Some("/var/log/test.log".to_string()),
            ..Default::default()
        };

        assert!(validate_service(&def).is_ok());
    }
}
