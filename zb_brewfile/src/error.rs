#[derive(Debug, thiserror::Error)]
pub enum BrewfileError {
    #[error("failed to parse Brewfile at line {line}: {message}")]
    ParseError { line: usize, message: String },

    #[error("failed to read Brewfile: {0}")]
    IoError(#[from] std::io::Error),

    #[error("installation failed: {0}")]
    InstallError(String),

    #[error("service error: {0}")]
    ServiceError(String),

    #[error("database error: {0}")]
    DatabaseError(#[from] zb_core::Error),
}
