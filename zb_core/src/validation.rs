//! Validation for formula identifiers (names, versions, dependencies)
//! Prevents path traversal, command injection, and other malicious inputs

use crate::errors::Error;

const MAX_IDENTIFIER_LENGTH: usize = 256;

/// Validate a formula name, version, or dependency name
///
/// Prevents:
/// - Path traversal (../, /, \)
/// - Null bytes
/// - Control characters
/// - Excessively long names
///
/// Allows:
/// - Alphanumeric characters
/// - Hyphens, underscores, plus signs
/// - At signs (for versioned formulas like openssl@3)
/// - Dots (for versions and extensions)
pub fn validate_identifier(name: &str, field: &str) -> Result<(), Error> {
    // Check length
    if name.is_empty() {
        return Err(Error::InvalidFormula {
            reason: format!("{} cannot be empty", field),
        });
    }

    if name.len() > MAX_IDENTIFIER_LENGTH {
        return Err(Error::InvalidFormula {
            reason: format!(
                "{} exceeds maximum length of {} characters",
                field, MAX_IDENTIFIER_LENGTH
            ),
        });
    }

    // Check for path traversal patterns
    if name.contains("..") {
        return Err(Error::InvalidFormula {
            reason: format!("{} contains path traversal sequence '..'", field),
        });
    }

    if name.contains('/') || name.contains('\\') {
        return Err(Error::InvalidFormula {
            reason: format!("{} contains path separator", field),
        });
    }

    // Check for null bytes
    if name.contains('\0') {
        return Err(Error::InvalidFormula {
            reason: format!("{} contains null byte", field),
        });
    }

    // Check for control characters (except newline which we'll reject specifically)
    if name.chars().any(|c| c.is_control()) {
        return Err(Error::InvalidFormula {
            reason: format!("{} contains control characters", field),
        });
    }

    // Validate character set
    let is_valid_char =
        |c: char| -> bool { c.is_alphanumeric() || matches!(c, '-' | '_' | '@' | '+' | '.' | ':') };

    if !name.chars().all(is_valid_char) {
        return Err(Error::InvalidFormula {
            reason: format!(
                "{} contains invalid characters (allowed: alphanumeric, -, _, @, +, ., :)",
                field
            ),
        });
    }

    // Don't allow leading/trailing dots or dashes
    if name.starts_with('.') || name.ends_with('.') {
        return Err(Error::InvalidFormula {
            reason: format!("{} cannot start or end with '.'", field),
        });
    }

    if name.starts_with('-') || name.ends_with('-') {
        return Err(Error::InvalidFormula {
            reason: format!("{} cannot start or end with '-'", field),
        });
    }

    Ok(())
}

/// Validate a formula name
pub fn validate_formula_name(name: &str) -> Result<(), Error> {
    validate_identifier(name, "formula name")
}

/// Validate a version string
pub fn validate_version(version: &str) -> Result<(), Error> {
    validate_identifier(version, "version")
}

/// Validate a dependency name
pub fn validate_dependency_name(name: &str) -> Result<(), Error> {
    validate_identifier(name, "dependency name")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_formula_names() {
        assert!(validate_formula_name("jq").is_ok());
        assert!(validate_formula_name("postgresql@15").is_ok());
        assert!(validate_formula_name("node").is_ok());
        assert!(validate_formula_name("gcc-13").is_ok());
        assert!(validate_formula_name("some-package_v2").is_ok());
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(validate_formula_name("../../etc/passwd").is_err());
        assert!(validate_formula_name("foo/../bar").is_err());
    }

    #[test]
    fn rejects_path_separators() {
        assert!(validate_formula_name("/etc/passwd").is_err());
        assert!(validate_formula_name("foo/bar").is_err());
        assert!(validate_formula_name("foo\\bar").is_err());
    }

    #[test]
    fn rejects_control_characters() {
        assert!(validate_formula_name("foo\nbar").is_err());
        assert!(validate_formula_name("foo\rbar").is_err());
        assert!(validate_formula_name("foo\tbar").is_err());
        assert!(validate_formula_name("foo\0bar").is_err());
    }

    #[test]
    fn rejects_special_characters() {
        assert!(validate_formula_name("foo;bar").is_err());
        assert!(validate_formula_name("foo$(cmd)").is_err());
        assert!(validate_formula_name("foo`cmd`").is_err());
        assert!(validate_formula_name("foo|bar").is_err());
    }

    #[test]
    fn rejects_leading_trailing_dots_dashes() {
        assert!(validate_formula_name(".hidden").is_err());
        assert!(validate_formula_name("name.").is_err());
        assert!(validate_formula_name("-prefixed").is_err());
        assert!(validate_formula_name("suffixed-").is_err());
    }

    #[test]
    fn rejects_empty_and_too_long() {
        assert!(validate_formula_name("").is_err());
        assert!(validate_formula_name(&"a".repeat(1000)).is_err());
    }

    #[test]
    fn validates_versions() {
        assert!(validate_version("1.0.0").is_ok());
        assert!(validate_version("2.5.1_3").is_ok());
        assert!(validate_version("../../../tmp").is_err());
    }

    #[test]
    fn validates_dependencies() {
        assert!(validate_dependency_name("openssl@3").is_ok());
        assert!(validate_dependency_name("../../etc/passwd").is_err());
    }
}
