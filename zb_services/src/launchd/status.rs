use regex::Regex;
use std::path::PathBuf;

use crate::error::ServiceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceState {
    Running,
    Stopped,
    Failed,
    Scheduled,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct LaunchctlStatus {
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub loaded_file: Option<PathBuf>,
    pub state: ServiceState,
}

impl LaunchctlStatus {
    pub fn parse_print_output(output: &str) -> Result<Self, ServiceError> {
        let pid_re = Regex::new(r"pid\s*=\s*(\d+)").unwrap();
        let exit_re = Regex::new(r"last exit code\s*=\s*(-?\d+)").unwrap();
        let path_re = Regex::new(r"path\s*=\s*(.+)").unwrap();

        let pid = pid_re
            .captures(output)
            .and_then(|caps| caps.get(1))
            .and_then(|m| m.as_str().parse().ok());

        let exit_code = exit_re
            .captures(output)
            .and_then(|caps| caps.get(1))
            .and_then(|m| m.as_str().parse().ok());

        let loaded_file = path_re
            .captures(output)
            .and_then(|caps| caps.get(1))
            .map(|m| PathBuf::from(m.as_str().trim()));

        let state = Self::determine_state(pid, exit_code);

        Ok(Self {
            pid,
            exit_code,
            loaded_file,
            state,
        })
    }

    pub fn parse_list_output(output: &str) -> Result<Self, ServiceError> {
        let pid_re = Regex::new(r#""PID"\s*=\s*(\d+);"#).unwrap();
        let exit_re = Regex::new(r#""LastExitStatus"\s*=\s*(-?\d+);"#).unwrap();

        let pid = pid_re
            .captures(output)
            .and_then(|caps| caps.get(1))
            .and_then(|m| m.as_str().parse().ok());

        let exit_code = exit_re
            .captures(output)
            .and_then(|caps| caps.get(1))
            .and_then(|m| m.as_str().parse().ok());

        let state = Self::determine_state(pid, exit_code);

        Ok(Self {
            pid,
            exit_code,
            loaded_file: None,
            state,
        })
    }

    fn determine_state(pid: Option<u32>, exit_code: Option<i32>) -> ServiceState {
        match (pid, exit_code) {
            (Some(p), _) if p > 0 => ServiceState::Running,
            (_, Some(0)) => ServiceState::Stopped,
            (_, Some(_)) => ServiceState::Failed,
            (None, None) => ServiceState::Stopped,
            _ => ServiceState::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_print_output_running() {
        let output = r#"
homebrew.mxcl.postgresql@15 = {
    active count = 1
    path = /Users/test/Library/LaunchAgents/homebrew.mxcl.postgresql@15.plist
    state = running
    pid = 12345
    last exit code = (never exited)
}
        "#;

        let status = LaunchctlStatus::parse_print_output(output).unwrap();
        assert_eq!(status.pid, Some(12345));
        assert_eq!(status.state, ServiceState::Running);
    }

    #[test]
    fn parses_print_output_stopped() {
        let output = r#"
homebrew.mxcl.test = {
    active count = 0
    path = /Users/test/Library/LaunchAgents/homebrew.mxcl.test.plist
    state = not running
    last exit code = 0
}
        "#;

        let status = LaunchctlStatus::parse_print_output(output).unwrap();
        assert_eq!(status.pid, None);
        assert_eq!(status.exit_code, Some(0));
        assert_eq!(status.state, ServiceState::Stopped);
    }

    #[test]
    fn parses_list_output_running() {
        let output = r#"{
    "Label" = "homebrew.mxcl.postgresql@15";
    "PID" = 12345;
    "LastExitStatus" = 0;
};"#;

        let status = LaunchctlStatus::parse_list_output(output).unwrap();
        assert_eq!(status.pid, Some(12345));
        assert_eq!(status.state, ServiceState::Running);
    }

    #[test]
    fn parses_list_output_failed() {
        let output = r#"{
    "Label" = "homebrew.mxcl.test";
    "LastExitStatus" = 1;
};"#;

        let status = LaunchctlStatus::parse_list_output(output).unwrap();
        assert_eq!(status.exit_code, Some(1));
        assert_eq!(status.state, ServiceState::Failed);
    }
}
