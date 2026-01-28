use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    UnsupportedBottle { name: String },
    ChecksumMismatch { expected: String, actual: String },
    LinkConflict { path: PathBuf },
    StoreCorruption { message: String },
    NetworkFailure { message: String },
    MissingFormula { name: String },
    UnsupportedTap { name: String },
    DependencyCycle { cycle: Vec<String> },
    NotInstalled { name: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnsupportedBottle { name } => {
                write!(f, "unsupported bottle for formula '{name}'")
            }
            Error::ChecksumMismatch { expected, actual } => {
                write!(f, "checksum mismatch (expected {expected}, got {actual})")
            }
            Error::LinkConflict { path } => {
                write!(f, "link conflict at '{}'", path.to_string_lossy())
            }
            Error::StoreCorruption { message } => write!(f, "store corruption: {message}"),
            Error::NetworkFailure { message } => write!(f, "network failure: {message}"),
            Error::MissingFormula { name } => write!(f, "missing formula '{name}'"),
            Error::UnsupportedTap { name } => {
                write!(
                    f,
                    "tap formula '{name}' is not supported (only homebrew/core)"
                )
            }
            Error::DependencyCycle { cycle } => {
                let rendered = cycle.join(" -> ");
                write!(f, "dependency cycle detected: {rendered}")
            }
            Error::NotInstalled { name } => write!(f, "formula '{name}' is not installed"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_bottle_display_includes_name() {
        let err = Error::UnsupportedBottle {
            name: "libheif".to_string(),
        };

        assert!(err.to_string().contains("libheif"));
    }
}
