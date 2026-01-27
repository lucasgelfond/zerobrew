use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDefinition {
    pub formula_name: String,
    pub plist_name: String,
    pub run: RunCommand,
    #[serde(default)]
    pub run_type: RunType,
    #[serde(default)]
    pub keep_alive: KeepAlive,
    pub working_dir: Option<String>,
    pub root_dir: Option<String>,
    pub log_path: Option<String>,
    pub error_log_path: Option<String>,
    #[serde(default)]
    pub environment_variables: HashMap<String, String>,
    #[serde(default = "default_run_at_load")]
    pub run_at_load: bool,
    #[serde(default)]
    pub launch_only_once: bool,
    #[serde(default)]
    pub require_root: bool,
    pub process_type: Option<ProcessType>,
    pub nice: Option<i32>,
    pub restart_delay: Option<u64>,
    pub throttle_interval: Option<u64>,
    pub interval: Option<u64>,
    pub cron: Option<CronSchedule>,
    #[serde(default)]
    pub sockets: HashMap<String, String>,
    #[serde(default)]
    pub macos_legacy_timers: bool,
}

fn default_run_at_load() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RunCommand {
    Simple(Vec<String>),
    PlatformSpecific {
        #[serde(skip_serializing_if = "Option::is_none")]
        macos: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        linux: Option<Vec<String>>,
    },
}

impl RunCommand {
    pub fn macos_command(&self) -> Option<&[String]> {
        match self {
            RunCommand::Simple(cmd) => Some(cmd),
            RunCommand::PlatformSpecific { macos, .. } => macos.as_deref(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunType {
    #[default]
    Immediate,
    Interval,
    Cron,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeepAlive {
    pub always: Option<bool>,
    pub successful_exit: Option<bool>,
    pub crashed: Option<bool>,
    pub path: Option<String>,
}

impl KeepAlive {
    pub fn is_enabled(&self) -> bool {
        self.always == Some(true)
            || self.successful_exit.is_some()
            || self.crashed.is_some()
            || self.path.is_some()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProcessType {
    Background,
    Standard,
    Interactive,
    Adaptive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronSchedule {
    #[serde(rename = "Minute")]
    pub minute: CronValue,
    #[serde(rename = "Hour")]
    pub hour: CronValue,
    #[serde(rename = "Day")]
    pub day: CronValue,
    #[serde(rename = "Month")]
    pub month: CronValue,
    #[serde(rename = "Weekday")]
    pub weekday: CronValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum CronValue {
    Any(String), // "*"
    Specific(u32),
}

impl CronValue {
    pub fn is_any(&self) -> bool {
        matches!(self, CronValue::Any(s) if s == "*")
    }
}

#[derive(Debug, Clone)]
pub struct SocketConfig {
    pub host: String,
    pub port: u16,
    pub protocol: SocketProtocol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketProtocol {
    Tcp,
    Udp,
}

impl SocketConfig {
    /// Parse socket string like "tcp://127.0.0.1:8080"
    pub fn parse(s: &str) -> Result<Self, String> {
        let re = regex::Regex::new(r"^(?P<proto>[a-z]+)://(?P<host>.+):(?P<port>[0-9]+)$")
            .map_err(|e| e.to_string())?;

        let caps = re
            .captures(s)
            .ok_or_else(|| format!("invalid socket format: {}", s))?;

        let protocol = match &caps["proto"] {
            "tcp" => SocketProtocol::Tcp,
            "udp" => SocketProtocol::Udp,
            proto => return Err(format!("unsupported protocol: {}", proto)),
        };

        let host = caps["host"].to_string();
        let port: u16 = caps["port"]
            .parse()
            .map_err(|e| format!("invalid port: {}", e))?;

        // Validate IP address
        use std::net::IpAddr;
        host.parse::<IpAddr>()
            .map_err(|_| format!("invalid IP address: {}", host))?;

        Ok(SocketConfig {
            host,
            port,
            protocol,
        })
    }
}

impl Default for ServiceDefinition {
    fn default() -> Self {
        Self {
            formula_name: String::new(),
            plist_name: String::new(),
            run: RunCommand::Simple(Vec::new()),
            run_type: RunType::default(),
            keep_alive: KeepAlive::default(),
            working_dir: None,
            root_dir: None,
            log_path: None,
            error_log_path: None,
            environment_variables: HashMap::new(),
            run_at_load: true,
            launch_only_once: false,
            require_root: false,
            process_type: None,
            nice: None,
            restart_delay: None,
            throttle_interval: None,
            interval: None,
            cron: None,
            sockets: HashMap::new(),
            macos_legacy_timers: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tcp_socket() {
        let socket = SocketConfig::parse("tcp://127.0.0.1:8080").unwrap();
        assert_eq!(socket.host, "127.0.0.1");
        assert_eq!(socket.port, 8080);
        assert_eq!(socket.protocol, SocketProtocol::Tcp);
    }

    #[test]
    fn parse_udp_socket() {
        let socket = SocketConfig::parse("udp://0.0.0.0:53").unwrap();
        assert_eq!(socket.host, "0.0.0.0");
        assert_eq!(socket.port, 53);
        assert_eq!(socket.protocol, SocketProtocol::Udp);
    }

    #[test]
    fn reject_invalid_ip() {
        assert!(SocketConfig::parse("tcp://invalid:8080").is_err());
    }

    #[test]
    fn reject_invalid_protocol() {
        assert!(SocketConfig::parse("http://127.0.0.1:8080").is_err());
    }

    #[test]
    fn deserialize_service_definition() {
        let json = r#"{
            "formula_name": "test",
            "plist_name": "homebrew.mxcl.test",
            "run": ["/usr/bin/test"],
            "run_type": "immediate",
            "keep_alive": {"always": true}
        }"#;

        let def: ServiceDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(def.formula_name, "test");
        assert!(def.run_at_load);
        assert_eq!(def.run_type, RunType::Immediate);
    }
}
