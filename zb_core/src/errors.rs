use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    UnsupportedBottle {
        name: String,
    },
    ChecksumMismatch {
        expected: String,
        actual: String,
    },
    LinkConflict {
        path: PathBuf,
    },
    StoreCorruption {
        message: String,
    },
    NetworkFailure {
        message: String,
    },
    MissingFormula {
        name: String,
    },
    MissingFormulaInSources {
        name: String,
        sources: Vec<String>,
    },
    UnsupportedTap {
        name: String,
    },
    DependencyCycle {
        cycle: Vec<String>,
    },
    NotInstalled {
        name: String,
    },
    FileError {
        message: String,
    },
    InvalidArgument {
        message: String,
    },
    ExecutionError {
        message: String,
    },
    InvalidTap {
        tap: String,
    },
    InvalidFormulaRef {
        reference: String,
    },
    ConflictingFormulaSource {
        name: String,
        first: String,
        second: String,
    },
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
            Error::MissingFormulaInSources { name, sources } => {
                if sources.is_empty() {
                    write!(f, "missing formula '{name}'")
                } else {
                    write!(
                        f,
                        "missing formula '{name}' (tried: {})",
                        sources.join(", ")
                    )
                }
            }
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
            Error::FileError { message } => write!(f, "file error: {message}"),
            Error::InvalidArgument { message } => write!(f, "invalid argument: {message}"),
            Error::ExecutionError { message } => write!(f, "{message}"),
            Error::InvalidTap { tap } => write!(f, "invalid tap '{tap}'"),
            Error::InvalidFormulaRef { reference } => {
                write!(f, "invalid formula reference '{reference}'")
            }
            Error::ConflictingFormulaSource {
                name,
                first,
                second,
            } => write!(
                f,
                "formula '{name}' resolved from multiple taps ({first} vs {second})"
            ),
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

    #[test]
    fn invalid_tap_display_includes_tap() {
        let err = Error::InvalidTap {
            tap: "user/".to_string(),
        };

        assert_eq!(err.to_string(), "invalid tap 'user/'");
    }

    #[test]
    fn invalid_formula_ref_display_includes_reference() {
        let err = Error::InvalidFormulaRef {
            reference: "user/tools/".to_string(),
        };

        assert_eq!(err.to_string(), "invalid formula reference 'user/tools/'");
    }

    #[test]
    fn conflicting_formula_source_display_is_specific() {
        let err = Error::ConflictingFormulaSource {
            name: "foo".to_string(),
            first: "tap user/tools".to_string(),
            second: "core".to_string(),
        };

        assert_eq!(
            err.to_string(),
            "formula 'foo' resolved from multiple taps (tap user/tools vs core)"
        );
    }

    #[test]
    fn missing_formula_in_sources_lists_sources() {
        let err = Error::MissingFormulaInSources {
            name: "foo".to_string(),
            sources: vec!["tap user/tools".to_string(), "core".to_string()],
        };

        assert_eq!(
            err.to_string(),
            "missing formula 'foo' (tried: tap user/tools, core)"
        );
    }
}
