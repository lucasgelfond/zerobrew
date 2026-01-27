use std::collections::HashMap;
use std::path::{Path, PathBuf};

use plist::{Dictionary, Value};

use crate::definition::{ProcessType, RunCommand, RunType, ServiceDefinition, SocketProtocol};
use crate::error::ServiceError;
use crate::validation::{validate_executable, validate_path};

pub struct PlistGenerator;

impl PlistGenerator {
    pub fn generate(def: &ServiceDefinition, prefix: &Path) -> Result<String, ServiceError> {
        let mut dict = Dictionary::new();

        // Required fields
        dict.insert("Label".to_string(), Value::String(def.plist_name.clone()));

        let command = Self::expand_run_command(&def.run, prefix)?;
        dict.insert("ProgramArguments".to_string(), Self::string_array(command));

        dict.insert("RunAtLoad".to_string(), Value::Boolean(def.run_at_load));

        // Optional fields
        if def.launch_only_once {
            dict.insert("LaunchOnlyOnce".to_string(), Value::Boolean(true));
        }

        if def.macos_legacy_timers {
            dict.insert("LegacyTimers".to_string(), Value::Boolean(true));
        }

        if let Some(delay) = def.restart_delay {
            dict.insert("TimeOut".to_string(), Value::Integer(delay.into()));
        }

        if let Some(throttle) = def.throttle_interval {
            dict.insert(
                "ThrottleInterval".to_string(),
                Value::Integer(throttle.into()),
            );
        }

        if let Some(ref process_type) = def.process_type {
            let pt_str = match process_type {
                ProcessType::Background => "Background",
                ProcessType::Standard => "Standard",
                ProcessType::Interactive => "Interactive",
                ProcessType::Adaptive => "Adaptive",
            };
            dict.insert("ProcessType".to_string(), Value::String(pt_str.to_string()));
        }

        if let Some(nice) = def.nice {
            dict.insert("Nice".to_string(), Value::Integer(nice.into()));
        }

        // Working directory
        if let Some(ref wd) = def.working_dir {
            validate_path(wd, "working_dir")?;
            let expanded = Self::expand_path(wd, prefix)?;
            dict.insert(
                "WorkingDirectory".to_string(),
                Value::String(expanded.to_string_lossy().to_string()),
            );
        }

        // Root directory
        if let Some(ref rd) = def.root_dir {
            validate_path(rd, "root_dir")?;
            let expanded = Self::expand_path(rd, prefix)?;
            dict.insert(
                "RootDirectory".to_string(),
                Value::String(expanded.to_string_lossy().to_string()),
            );
        }

        // Log paths
        if let Some(ref lp) = def.log_path {
            validate_path(lp, "log_path")?;
            let expanded = Self::expand_path(lp, prefix)?;
            dict.insert(
                "StandardOutPath".to_string(),
                Value::String(expanded.to_string_lossy().to_string()),
            );
        }

        if let Some(ref elp) = def.error_log_path {
            validate_path(elp, "error_log_path")?;
            let expanded = Self::expand_path(elp, prefix)?;
            dict.insert(
                "StandardErrorPath".to_string(),
                Value::String(expanded.to_string_lossy().to_string()),
            );
        }

        // Environment variables
        if !def.environment_variables.is_empty() {
            let mut env_dict = Dictionary::new();
            for (k, v) in &def.environment_variables {
                env_dict.insert(k.to_string(), Value::String(v.to_string()));
            }
            dict.insert(
                "EnvironmentVariables".to_string(),
                Value::Dictionary(env_dict),
            );
        }

        // Keep alive
        if def.keep_alive.is_enabled() {
            Self::add_keep_alive(&mut dict, &def.keep_alive, prefix)?;
        }

        // Timers
        match def.run_type {
            RunType::Interval => {
                if let Some(interval) = def.interval {
                    dict.insert("StartInterval".to_string(), Value::Integer(interval.into()));
                }
            }
            RunType::Cron => {
                if let Some(ref cron) = def.cron {
                    dict.insert(
                        "StartCalendarInterval".to_string(),
                        Self::cron_to_dict(cron),
                    );
                }
            }
            RunType::Immediate => {}
        }

        // Sockets
        if !def.sockets.is_empty() {
            dict.insert("Sockets".to_string(), Self::sockets_to_dict(&def.sockets)?);
        }

        // Session types
        dict.insert(
            "LimitLoadToSessionType".to_string(),
            Self::string_array(vec![
                "Aqua".to_string(),
                "Background".to_string(),
                "LoginWindow".to_string(),
                "StandardIO".to_string(),
                "System".to_string(),
            ]),
        );

        // Convert to XML
        let value = Value::Dictionary(dict);
        let mut buf = Vec::new();
        value.to_writer_xml(&mut buf)?;

        String::from_utf8(buf).map_err(|e| {
            ServiceError::InvalidDefinition(format!("plist contains invalid UTF-8: {}", e))
        })
    }

    fn expand_run_command(run: &RunCommand, prefix: &Path) -> Result<Vec<String>, ServiceError> {
        let command = match run {
            RunCommand::Simple(cmd) => cmd.clone(),
            RunCommand::PlatformSpecific { macos, .. } => macos.clone().ok_or_else(|| {
                ServiceError::InvalidDefinition("no macOS run command".to_string())
            })?,
        };

        let mut expanded = Vec::new();

        for arg in command {
            let expanded_arg = Self::expand_path_string(&arg, prefix)?;
            expanded.push(expanded_arg);
        }

        // Validate first element (executable) exists and is executable
        if let Some(exe) = expanded.first() {
            let exe_path = PathBuf::from(exe);
            validate_executable(&exe_path)?;
        }

        Ok(expanded)
    }

    fn expand_path(path_str: &str, prefix: &Path) -> Result<PathBuf, ServiceError> {
        let expanded = Self::expand_path_string(path_str, prefix)?;
        Ok(PathBuf::from(expanded))
    }

    fn expand_path_string(s: &str, prefix: &Path) -> Result<String, ServiceError> {
        let mut result = s.to_string();

        // Expand ~ to user home
        if result.starts_with('~')
            && let Ok(home) = std::env::var("HOME")
        {
            result = result.replacen('~', &home, 1);
        }

        // Expand common Homebrew paths
        result = result.replace("/opt/homebrew/prefix", &prefix.to_string_lossy());
        result = result.replace("/opt/homebrew/var", &prefix.join("var").to_string_lossy());
        result = result.replace("/opt/homebrew/etc", &prefix.join("etc").to_string_lossy());
        result = result.replace("/opt/homebrew/opt", &prefix.join("opt").to_string_lossy());

        Ok(result)
    }

    fn add_keep_alive(
        dict: &mut Dictionary,
        keep_alive: &crate::definition::KeepAlive,
        prefix: &Path,
    ) -> Result<(), ServiceError> {
        if let Some(always) = keep_alive.always {
            dict.insert("KeepAlive".to_string(), Value::Boolean(always));
        } else if let Some(successful_exit) = keep_alive.successful_exit {
            let mut ka_dict = Dictionary::new();
            ka_dict.insert(
                "SuccessfulExit".to_string(),
                Value::Boolean(successful_exit),
            );
            dict.insert("KeepAlive".to_string(), Value::Dictionary(ka_dict));
        } else if let Some(crashed) = keep_alive.crashed {
            let mut ka_dict = Dictionary::new();
            ka_dict.insert("Crashed".to_string(), Value::Boolean(crashed));
            dict.insert("KeepAlive".to_string(), Value::Dictionary(ka_dict));
        } else if let Some(ref path) = keep_alive.path {
            validate_path(path, "keep_alive.path")?;
            let expanded = Self::expand_path(path, prefix)?;
            let mut ka_dict = Dictionary::new();
            ka_dict.insert(
                "PathState".to_string(),
                Value::String(expanded.to_string_lossy().to_string()),
            );
            dict.insert("KeepAlive".to_string(), Value::Dictionary(ka_dict));
        }

        Ok(())
    }

    fn cron_to_dict(cron: &crate::definition::CronSchedule) -> Value {
        use crate::definition::CronValue;
        let mut dict = Dictionary::new();

        if let CronValue::Specific(v) = &cron.minute {
            dict.insert("Minute".to_string(), Value::Integer((*v).into()));
        }

        if let CronValue::Specific(v) = &cron.hour {
            dict.insert("Hour".to_string(), Value::Integer((*v).into()));
        }

        if let CronValue::Specific(v) = &cron.day {
            dict.insert("Day".to_string(), Value::Integer((*v).into()));
        }

        if let CronValue::Specific(v) = &cron.month {
            dict.insert("Month".to_string(), Value::Integer((*v).into()));
        }

        if let CronValue::Specific(v) = &cron.weekday {
            dict.insert("Weekday".to_string(), Value::Integer((*v).into()));
        }

        Value::Dictionary(dict)
    }

    fn sockets_to_dict(sockets: &HashMap<String, String>) -> Result<Value, ServiceError> {
        let mut sockets_dict = Dictionary::new();

        for (name, socket_str) in sockets {
            let socket_config = crate::definition::SocketConfig::parse(socket_str)
                .map_err(ServiceError::InvalidDefinition)?;

            let mut socket_dict = Dictionary::new();
            socket_dict.insert(
                "SockNodeName".to_string(),
                Value::String(socket_config.host),
            );
            socket_dict.insert(
                "SockServiceName".to_string(),
                Value::String(socket_config.port.to_string()),
            );

            let proto = match socket_config.protocol {
                SocketProtocol::Tcp => "TCP",
                SocketProtocol::Udp => "UDP",
            };
            socket_dict.insert("SockProtocol".to_string(), Value::String(proto.to_string()));

            sockets_dict.insert(name.to_string(), Value::Dictionary(socket_dict));
        }

        Ok(Value::Dictionary(sockets_dict))
    }

    fn string_array(strings: Vec<String>) -> Value {
        Value::Array(strings.into_iter().map(Value::String).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::{KeepAlive, RunCommand, RunType};
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn create_test_prefix() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();

        // Create a fake executable
        let exe = bin.join("test-service");
        std::fs::write(&exe, "#!/bin/sh\necho test").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();

        tmp
    }

    #[test]
    fn generates_minimal_plist() {
        let tmp = create_test_prefix();
        let def = ServiceDefinition {
            formula_name: "test".to_string(),
            plist_name: "homebrew.mxcl.test".to_string(),
            run: RunCommand::Simple(vec![
                tmp.path()
                    .join("bin/test-service")
                    .to_string_lossy()
                    .to_string(),
            ]),
            ..Default::default()
        };

        let plist = PlistGenerator::generate(&def, tmp.path()).unwrap();

        assert!(plist.contains("homebrew.mxcl.test"));
        assert!(plist.contains("ProgramArguments"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
    }

    #[test]
    fn includes_keep_alive_always() {
        let tmp = create_test_prefix();
        let keep_alive = KeepAlive {
            always: Some(true),
            ..Default::default()
        };

        let def = ServiceDefinition {
            formula_name: "test".to_string(),
            plist_name: "homebrew.mxcl.test".to_string(),
            run: RunCommand::Simple(vec![
                tmp.path()
                    .join("bin/test-service")
                    .to_string_lossy()
                    .to_string(),
            ]),
            keep_alive,
            ..Default::default()
        };

        let plist = PlistGenerator::generate(&def, tmp.path()).unwrap();

        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains("<true/>"));
    }

    #[test]
    fn includes_working_directory() {
        let tmp = create_test_prefix();
        let var_dir = tmp.path().join("var");
        std::fs::create_dir(&var_dir).unwrap();

        let def = ServiceDefinition {
            formula_name: "test".to_string(),
            plist_name: "homebrew.mxcl.test".to_string(),
            run: RunCommand::Simple(vec![
                tmp.path()
                    .join("bin/test-service")
                    .to_string_lossy()
                    .to_string(),
            ]),
            working_dir: Some(var_dir.to_string_lossy().to_string()),
            ..Default::default()
        };

        let plist = PlistGenerator::generate(&def, tmp.path()).unwrap();

        assert!(plist.contains("<key>WorkingDirectory</key>"));
        assert!(plist.contains(&var_dir.to_string_lossy().to_string()));
    }

    #[test]
    fn includes_interval_for_interval_type() {
        let tmp = create_test_prefix();
        let def = ServiceDefinition {
            formula_name: "test".to_string(),
            plist_name: "homebrew.mxcl.test".to_string(),
            run: RunCommand::Simple(vec![
                tmp.path()
                    .join("bin/test-service")
                    .to_string_lossy()
                    .to_string(),
            ]),
            run_type: RunType::Interval,
            interval: Some(300),
            ..Default::default()
        };

        let plist = PlistGenerator::generate(&def, tmp.path()).unwrap();

        assert!(plist.contains("<key>StartInterval</key>"));
        assert!(plist.contains("<integer>300</integer>"));
    }

    #[test]
    fn rejects_nonexistent_executable() {
        let tmp = TempDir::new().unwrap();
        let def = ServiceDefinition {
            formula_name: "test".to_string(),
            plist_name: "homebrew.mxcl.test".to_string(),
            run: RunCommand::Simple(vec!["/nonexistent/binary".to_string()]),
            ..Default::default()
        };

        assert!(PlistGenerator::generate(&def, tmp.path()).is_err());
    }
}
