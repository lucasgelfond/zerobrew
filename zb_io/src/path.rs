use std::path::{Component, Path};

use zb_core::Error;

const MAX_PATH_LEN: usize = 4096;

pub fn validate_privileged_path(path: &Path) -> Result<(), Error> {
    let path_str = path.to_string_lossy();

    if path_str.len() > MAX_PATH_LEN {
        return Err(Error::InvalidArgument {
            message: format!(
                "path exceeds maximum length of {MAX_PATH_LEN} bytes: {}",
                path.display()
            ),
        });
    }

    if path_str.bytes().any(|b| b.is_ascii_control()) {
        return Err(Error::InvalidArgument {
            message: "path contains control characters".to_string(),
        });
    }

    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(Error::InvalidArgument {
                message: format!("path contains '..' traversal: {}", path.display()),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn accepts_normal_absolute_path() {
        assert!(validate_privileged_path(Path::new("/opt/zerobrew")).is_ok());
    }

    #[test]
    fn accepts_normal_relative_path() {
        assert!(validate_privileged_path(Path::new("zerobrew/store")).is_ok());
    }

    #[test]
    fn rejects_parent_dir_traversal() {
        let err = validate_privileged_path(Path::new("/opt/../etc/shadow")).unwrap_err();
        assert!(err.to_string().contains("'..'"));
    }

    #[test]
    fn rejects_control_characters() {
        let bad = "/opt/zero\x07brew";
        let err = validate_privileged_path(Path::new(bad)).unwrap_err();
        assert!(err.to_string().contains("control characters"));
    }

    #[test]
    fn rejects_null_byte_in_path() {
        let bad = "/opt/zero\x00brew";
        let err = validate_privileged_path(Path::new(bad)).unwrap_err();
        assert!(err.to_string().contains("control characters"));
    }

    #[test]
    fn rejects_newline_in_path() {
        let bad = "/opt/zero\nbrew";
        let err = validate_privileged_path(Path::new(bad)).unwrap_err();
        assert!(err.to_string().contains("control characters"));
    }

    #[test]
    fn rejects_excessively_long_path() {
        let long = "/".to_string() + &"a".repeat(MAX_PATH_LEN + 1);
        let err = validate_privileged_path(Path::new(&long)).unwrap_err();
        assert!(err.to_string().contains("exceeds maximum length"));
    }

    #[test]
    fn accepts_path_at_max_length() {
        let long = "/".to_string() + &"a".repeat(MAX_PATH_LEN - 1);
        assert!(validate_privileged_path(Path::new(&long)).is_ok());
    }

    #[test]
    fn rejects_trailing_dotdot() {
        let err = validate_privileged_path(Path::new("/opt/zerobrew/..")).unwrap_err();
        assert!(err.to_string().contains("'..'"));
    }
}
