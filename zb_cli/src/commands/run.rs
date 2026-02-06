use console::style;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use zb_io::Installer;

use crate::utils::normalize_formula_name;

/// Prepare a package for execution by ensuring it's installed
/// Returns the path to the executable
pub async fn prepare_execution(
    installer: &mut Installer,
    formula: &str,
) -> Result<PathBuf, zb_core::Error> {
    let normalized = normalize_formula_name(formula)?;

    let was_installed = installer.is_installed(&normalized);

    if !was_installed {
        println!(
            "{} Installing {} temporarily...",
            style("==>").cyan().bold(),
            style(&normalized).green()
        );

        let plan = installer.plan(std::slice::from_ref(&normalized)).await?;
        installer.execute(plan, false).await?;
    }

    let installed =
        installer
            .get_installed(&normalized)
            .ok_or_else(|| zb_core::Error::NotInstalled {
                name: normalized.clone(),
            })?;

    let keg_path = installer.keg_path(&normalized, &installed.version);
    let bin_path = keg_path.join("bin").join(&normalized);

    if !bin_path.exists() {
        return Err(zb_core::Error::ExecutionError {
            message: format!(
                "executable '{}' not found in package '{}'",
                normalized, normalized
            ),
        });
    }

    Ok(bin_path)
}

pub async fn execute(
    installer: &mut Installer,
    formula: String,
    args: Vec<String>,
) -> Result<(), zb_core::Error> {
    println!(
        "{} Running {}...",
        style("==>").cyan().bold(),
        style(&formula).bold()
    );

    let bin_path = prepare_execution(installer, &formula).await?;

    println!(
        "{} Executing {}...",
        style("==>").cyan().bold(),
        style(&formula).green()
    );

    let mut cmd = Command::new(&bin_path);
    cmd.args(&args);

    if let Ok(prefix_str) = std::env::var("ZEROBREW_PREFIX") {
        let prefix_path = PathBuf::from(&prefix_str);

        if let Some(ca_bundle) = zb_io::find_ca_bundle_from_prefix(&prefix_path) {
            cmd.env("CURL_CA_BUNDLE", &ca_bundle);
            cmd.env("SSL_CERT_FILE", &ca_bundle);
        }

        if let Some(ca_dir) = zb_io::find_ca_dir(&prefix_path) {
            cmd.env("SSL_CERT_DIR", &ca_dir);
        }

        let lib_path = prefix_path.join("lib");
        if let Ok(existing_ld_path) = std::env::var("LD_LIBRARY_PATH") {
            cmd.env(
                "LD_LIBRARY_PATH",
                format!("{}:{}", lib_path.display(), existing_ld_path),
            );
        } else {
            cmd.env("LD_LIBRARY_PATH", lib_path);
        }
    }

    let err = cmd.exec();

    Err(zb_core::Error::ExecutionError {
        message: format!("failed to execute '{}': {}", formula, err),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use zb_io::{ApiClient, BlobCache, Cellar, Database, Linker, Store};

    fn create_bottle_tarball(formula_name: &str) -> Vec<u8> {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::io::Write;
        use tar::Builder;

        let mut builder = Builder::new(Vec::new());

        let content = format!("#!/bin/sh\necho {}", formula_name);
        let content_bytes = content.as_bytes();

        let mut header = tar::Header::new_gnu();
        header
            .set_path(format!("{}/1.0.0/bin/{}", formula_name, formula_name))
            .unwrap();
        header.set_size(content_bytes.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();

        builder.append(&header, content_bytes).unwrap();

        let tar_data = builder.into_inner().unwrap();

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&tar_data).unwrap();
        encoder.finish().unwrap()
    }

    fn sha256_hex(data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    fn get_test_bottle_tag() -> &'static str {
        if cfg!(target_os = "linux") {
            "x86_64_linux"
        } else if cfg!(target_arch = "x86_64") {
            "sonoma"
        } else {
            "arm64_sonoma"
        }
    }

    #[tokio::test]
    async fn run_installs_package_if_not_present() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        let bottle = create_bottle_tarball("testrun");
        let bottle_sha = sha256_hex(&bottle);

        let tag = get_test_bottle_tag();
        let formula_json = format!(
            r#"{{
                "name": "testrun",
                "versions": {{ "stable": "1.0.0" }},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "{}": {{
                                "url": "{}/bottles/testrun.tar.gz",
                                "sha256": "{}"
                            }}
                        }}
                    }}
                }}
            }}"#,
            tag,
            mock_server.uri(),
            bottle_sha
        );

        Mock::given(method("GET"))
            .and(path("/testrun.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&formula_json))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/bottles/testrun.tar.gz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bottle.clone()))
            .mount(&mock_server)
            .await;

        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client = ApiClient::with_base_url(mock_server.uri());
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let mut installer = Installer::new(api_client, blob_cache, store, cellar, linker, db);

        assert!(!installer.is_installed("testrun"));

        let bin_path = prepare_execution(&mut installer, "testrun").await.unwrap();

        assert!(installer.is_installed("testrun"));
        assert!(!prefix.join("bin/testrun").exists());

        assert!(bin_path.exists());
        assert!(bin_path.ends_with("bin/testrun"));

        let output = std::process::Command::new(&bin_path).output().unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "testrun");
    }

    #[tokio::test]
    async fn run_reuses_already_installed_package() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        let bottle = create_bottle_tarball("alreadyinstalled");
        let bottle_sha = sha256_hex(&bottle);

        let tag = get_test_bottle_tag();
        let formula_json = format!(
            r#"{{
                "name": "alreadyinstalled",
                "versions": {{ "stable": "1.0.0" }},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "{}": {{
                                "url": "{}/bottles/alreadyinstalled.tar.gz",
                                "sha256": "{}"
                            }}
                        }}
                    }}
                }}
            }}"#,
            tag,
            mock_server.uri(),
            bottle_sha
        );

        Mock::given(method("GET"))
            .and(path("/alreadyinstalled.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&formula_json))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/bottles/alreadyinstalled.tar.gz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bottle.clone()))
            .mount(&mock_server)
            .await;

        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client = ApiClient::with_base_url(mock_server.uri());
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let mut installer = Installer::new(api_client, blob_cache, store, cellar, linker, db);

        installer
            .install(&["alreadyinstalled".to_string()], false)
            .await
            .unwrap();
        assert!(installer.is_installed("alreadyinstalled"));

        let bin_path = prepare_execution(&mut installer, "alreadyinstalled")
            .await
            .unwrap();

        assert!(bin_path.exists());
        assert!(bin_path.ends_with("bin/alreadyinstalled"));

        let output = std::process::Command::new(&bin_path).output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "alreadyinstalled"
        );
    }

    #[tokio::test]
    async fn run_fails_for_missing_formula() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        Mock::given(method("GET"))
            .and(path("/nonexistent.json"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client = ApiClient::with_base_url(mock_server.uri());
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let mut installer = Installer::new(api_client, blob_cache, store, cellar, linker, db);

        let result = prepare_execution(&mut installer, "nonexistent").await;
        assert!(result.is_err());
    }

    #[test]
    fn ssl_cert_paths_use_prefix() {
        let prefix = "/opt/test/prefix";
        let ca_bundle = format!(
            "{}/opt/ca-certificates/share/ca-certificates/cacert.pem",
            prefix
        );
        let ca_dir = format!("{}/etc/ca-certificates", prefix);

        assert_eq!(
            ca_bundle,
            "/opt/test/prefix/opt/ca-certificates/share/ca-certificates/cacert.pem"
        );
        assert_eq!(ca_dir, "/opt/test/prefix/etc/ca-certificates");
    }
}
