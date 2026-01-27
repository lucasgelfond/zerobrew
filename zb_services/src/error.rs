use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("formula '{0}' is not installed")]
    FormulaNotInstalled(String),

    #[error("formula '{0}' has no service definition")]
    NoServiceDefinition(String),

    #[error("service '{0}' is not registered")]
    ServiceNotRegistered(String),

    #[error("launchctl {command} failed: {stderr}")]
    LaunchctlFailed { command: String, stderr: String },

    #[error("invalid plist location: {0}")]
    InvalidPlistLocation(PathBuf),

    #[error("symlink attack detected: {0}")]
    SymlinkAttack(PathBuf),

    #[error("path traversal detected in: {0}")]
    PathTraversal(String),

    #[error("executable not found: {0}")]
    ExecutableNotFound(PathBuf),

    #[error("executable not executable: {0}")]
    NotExecutable(PathBuf),

    #[error("invalid service definition: {0}")]
    InvalidDefinition(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("plist error: {0}")]
    Plist(#[from] plist::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("database error: {0}")]
    Database(#[from] zb_core::Error),
}
