use crate::storage::db::Database;
use std::path::{Path, PathBuf};

pub fn find_ca_bundle(prefix: &Path, db: &Database) -> Option<PathBuf> {
    let installed = db.get_installed("ca-certificates")?;

    let keg_path = prefix
        .join("Cellar")
        .join("ca-certificates")
        .join(&installed.version);

    let candidates = [
        keg_path.join("share/ca-certificates/cacert.pem"),
        keg_path.join("share/ca-bundle.crt"),
        keg_path.join("etc/openssl/cert.pem"),
        keg_path.join("ssl/cert.pem"),
    ];

    candidates.into_iter().find(|p| p.exists())
}

pub fn find_ca_bundle_from_prefix(prefix: &Path) -> Option<PathBuf> {
    let candidates = [
        prefix.join("opt/ca-certificates/share/ca-certificates/cacert.pem"),
        prefix.join("etc/ca-certificates/cacert.pem"),
        prefix.join("etc/openssl/cert.pem"),
        prefix.join("share/ca-certificates/cacert.pem"),
    ];

    candidates.into_iter().find(|p| p.exists())
}

pub fn find_ca_dir(prefix: &Path) -> Option<PathBuf> {
    let candidates = [
        prefix.join("etc/ca-certificates"),
        prefix.join("etc/openssl/certs"),
        prefix.join("share/ca-certificates"),
    ];

    candidates.into_iter().find(|p| p.exists() && p.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn finds_ca_bundle_in_standard_location() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path().join("prefix");
        let ca_path = prefix.join("opt/ca-certificates/share/ca-certificates");
        fs::create_dir_all(&ca_path).unwrap();
        fs::write(ca_path.join("cacert.pem"), b"cert").unwrap();

        let found = find_ca_bundle_from_prefix(&prefix);
        assert!(found.is_some());
        assert!(found.unwrap().ends_with("cacert.pem"));
    }

    #[test]
    fn finds_ca_bundle_in_alternative_location() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path().join("prefix");
        let ca_path = prefix.join("etc/ca-certificates");
        fs::create_dir_all(&ca_path).unwrap();
        fs::write(ca_path.join("cacert.pem"), b"cert").unwrap();

        let found = find_ca_bundle_from_prefix(&prefix);
        assert!(found.is_some());
    }

    #[test]
    fn returns_none_when_no_bundle_exists() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path().join("prefix");
        fs::create_dir_all(&prefix).unwrap();

        let found = find_ca_bundle_from_prefix(&prefix);
        assert!(found.is_none());
    }

    #[test]
    fn finds_ca_dir() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path().join("prefix");
        let ca_dir = prefix.join("etc/ca-certificates");
        fs::create_dir_all(&ca_dir).unwrap();

        let found = find_ca_dir(&prefix);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), ca_dir);
    }
}
