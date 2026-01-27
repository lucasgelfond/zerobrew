pub mod definition;
pub mod error;
pub mod launchd;
pub mod manager;
pub mod validation;

#[cfg(test)]
mod security_tests;

pub use definition::{
    CronSchedule, CronValue, KeepAlive, ProcessType, RunCommand, RunType, ServiceDefinition,
    SocketConfig,
};
pub use error::ServiceError;
pub use launchd::{LaunchctlStatus, LaunchdManager, ServiceState};
pub use manager::{ServiceManager, ServiceStatus};
pub use validation::validate_service;
