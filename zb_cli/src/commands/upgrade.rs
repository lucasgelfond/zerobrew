use std::collections::BTreeSet;
use std::time::Instant;

use console::style;

use crate::utils::normalize_formula_name;

pub async fn execute(
    installer: &mut zb_io::Installer,
    packages: Vec<String>,
    build_from_source: bool,
) -> Result<(), zb_core::Error> {
    let start = Instant::now();
    let mut targets = Vec::new();
    let mut seen = BTreeSet::new();
    let mut failures: Vec<(String, zb_core::Error)> = Vec::new();

    if packages.is_empty() {
        let (outdated, warnings) = installer.check_outdated().await?;
        for warning in &warnings {
            eprintln!("{} {}", style("Warning:").yellow().bold(), warning);
        }
        for pkg in outdated {
            if seen.insert(pkg.name.clone()) {
                targets.push(pkg.name);
            }
        }
        if targets.is_empty() {
            println!(
                "{} All packages are up to date.",
                style("==>").cyan().bold()
            );
            return Ok(());
        }
    } else {
        for package in packages {
            let normalized = match normalize_formula_name(&package) {
                Ok(name) => name,
                Err(e) => {
                    failures.push((package, e));
                    continue;
                }
            };

            if !seen.insert(normalized.clone()) {
                continue;
            }

            match installer.is_outdated(&normalized).await {
                Ok(Some(_)) => targets.push(normalized),
                Ok(None) => {
                    println!(
                        "{} {} is already up to date.",
                        style("==>").cyan().bold(),
                        style(normalized).bold()
                    );
                }
                Err(e) => failures.push((normalized, e)),
            }
        }

        if targets.is_empty() && failures.is_empty() {
            println!(
                "{} All selected packages are up to date.",
                style("==>").cyan().bold()
            );
            return Ok(());
        }
    }

    let total = targets.len() + failures.len();
    println!(
        "{} Upgrading {} package(s)...",
        style("==>").cyan().bold(),
        style(targets.len()).green().bold()
    );

    let mut upgraded = 0usize;

    for target in targets {
        print!("    {} {}...", style("○").dim(), target);

        let before = match installer.get_installed(&target) {
            Some(before) => before,
            None => {
                println!(" {}", style("✗").red());
                failures.push((
                    target.clone(),
                    zb_core::Error::NotInstalled {
                        name: target.clone(),
                    },
                ));
                continue;
            }
        };

        let result = if target.starts_with("cask:") {
            if build_from_source {
                eprintln!(
                    "{} --build-from-source is ignored for cask {}",
                    style("Warning:").yellow().bold(),
                    style(&target).bold()
                );
            }
            installer
                .install_casks(std::slice::from_ref(&target), true)
                .await
                .map(|_| ())
        } else {
            let plan = match installer
                .plan_with_options(std::slice::from_ref(&target), build_from_source)
                .await
            {
                Ok(plan) => plan,
                Err(e) => {
                    println!(" {}", style("✗").red());
                    failures.push((target.clone(), e));
                    continue;
                }
            };
            installer.execute(plan, true).await.map(|_| ())
        };

        if let Err(e) = result {
            println!(" {}", style("✗").red());
            failures.push((target.clone(), e));
            continue;
        }

        let after = match installer.get_installed(&target) {
            Some(after) => after,
            None => {
                println!(" {}", style("✗").red());
                failures.push((
                    target.clone(),
                    zb_core::Error::NotInstalled {
                        name: target.clone(),
                    },
                ));
                continue;
            }
        };

        if before.version != after.version
            && let Err(e) = installer.remove_keg_version(&target, &before.version)
        {
            println!(" {}", style("✗").red());
            failures.push((target.clone(), e));
            continue;
        }

        println!(" {}", style("✓").green());
        upgraded += 1;
    }

    println!();
    println!(
        "{} Upgraded {} / {} package(s) in {:.2}s",
        style("==>").cyan().bold(),
        style(upgraded).green().bold(),
        total,
        start.elapsed().as_secs_f64()
    );

    if failures.is_empty() {
        return Ok(());
    }

    eprintln!(
        "{} {} package(s) failed to upgrade:",
        style("Error:").red().bold(),
        failures.len()
    );
    for (name, error) in &failures {
        eprintln!("    {} {}: {}", style("✗").red(), style(name).bold(), error);
    }

    Err(zb_core::Error::ExecutionError {
        message: format!("failed to upgrade {} package(s)", failures.len()),
    })
}

#[cfg(test)]
mod tests {
    use super::execute;
    use std::fs;
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use zb_io::network::api::ApiClient;
    use zb_io::storage::blob::BlobCache;
    use zb_io::storage::db::Database;
    use zb_io::storage::store::Store;
    use zb_io::{Cellar, Installer, Linker};

    fn create_bottle_tarball(formula_name: &str, version: &str) -> Vec<u8> {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::io::Write;
        use tar::Builder;

        let mut builder = Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header
            .set_path(format!("{}/{}/bin/{}", formula_name, version, formula_name))
            .unwrap();
        header.set_size(20);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append(
                &header,
                format!("#!/bin/sh\necho {formula_name}").as_bytes(),
            )
            .unwrap();

        let tar_data = builder.into_inner().unwrap();
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&tar_data).unwrap();
        encoder.finish().unwrap()
    }

    fn create_cask_zip(binary_path: &str, contents: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::CompressionMethod;
        use zip::write::{SimpleFileOptions, ZipWriter};

        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer.start_file(binary_path, options).unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
        writer.finish().unwrap().into_inner()
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

    fn test_installer(
        root: &std::path::Path,
        prefix: &std::path::Path,
        base_url: &str,
    ) -> Installer {
        fs::create_dir_all(root.join("db")).unwrap();
        let api_client = ApiClient::with_base_url(format!("{base_url}/formula"))
            .unwrap()
            .with_cask_base_url(base_url.to_string());
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(root).unwrap();
        let cellar = Cellar::new(root).unwrap();
        let linker = Linker::new(prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();
        Installer::new(
            api_client,
            blob_cache,
            store,
            cellar,
            linker,
            db,
            prefix.to_path_buf(),
        )
    }

    #[tokio::test]
    async fn upgrade_without_args_upgrades_formula_and_cask_and_prunes_old_kegs() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");

        let tag = get_test_bottle_tag();
        let old_bottle = create_bottle_tarball("jq", "1.0.0");
        let old_bottle_sha = sha256_hex(&old_bottle);
        let new_bottle = create_bottle_tarball("jq", "2.0.0");
        let new_bottle_sha = sha256_hex(&new_bottle);
        let old_cask_zip = create_cask_zip("bin/demo", "#!/bin/sh\necho old-demo\n");
        let old_cask_sha = sha256_hex(&old_cask_zip);
        let new_cask_zip = create_cask_zip("bin/demo", "#!/bin/sh\necho new-demo\n");
        let new_cask_sha = sha256_hex(&new_cask_zip);

        let old_formula_json = format!(
            r#"{{
                "name":"jq",
                "versions":{{"stable":"1.0.0"}},
                "dependencies":[],
                "bottle":{{"stable":{{"files":{{"{}":{{"url":"{}/bottles/jq-1.0.0.{}.bottle.tar.gz","sha256":"{}"}}}}}}}}
            }}"#,
            tag,
            mock_server.uri(),
            tag,
            old_bottle_sha
        );
        let new_formula_json = format!(
            r#"{{
                "name":"jq",
                "versions":{{"stable":"2.0.0"}},
                "dependencies":[],
                "bottle":{{"stable":{{"files":{{"{}":{{"url":"{}/bottles/jq-2.0.0.{}.bottle.tar.gz","sha256":"{}"}}}}}}}}
            }}"#,
            tag,
            mock_server.uri(),
            tag,
            new_bottle_sha
        );
        let new_formula_json_for_closure = new_formula_json.clone();
        let formula_calls = Arc::new(AtomicUsize::new(0));
        let formula_calls_clone = formula_calls.clone();
        Mock::given(method("GET"))
            .and(path("/formula/jq.json"))
            .respond_with(move |_req: &wiremock::Request| {
                let count = formula_calls_clone.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    ResponseTemplate::new(200).set_body_string(old_formula_json.clone())
                } else {
                    ResponseTemplate::new(200).set_body_string(new_formula_json_for_closure.clone())
                }
            })
            .mount(&mock_server)
            .await;

        let bulk_json = format!("[{}]", new_formula_json);
        Mock::given(method("GET"))
            .and(path("/formula.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(bulk_json))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("/bottles/jq-1.0.0.{}.bottle.tar.gz", tag)))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(old_bottle))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/bottles/jq-2.0.0.{}.bottle.tar.gz", tag)))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(new_bottle))
            .mount(&mock_server)
            .await;

        let old_cask_json = format!(
            r#"{{
                "token":"demo",
                "version":"1.0.0",
                "url":"{}/casks/demo-old.zip",
                "sha256":"{}",
                "artifacts":[{{"binary":[["bin/demo"]]}}]
            }}"#,
            mock_server.uri(),
            old_cask_sha
        );
        let new_cask_json = format!(
            r#"{{
                "token":"demo",
                "version":"2.0.0",
                "url":"{}/casks/demo-new.zip",
                "sha256":"{}",
                "artifacts":[{{"binary":[["bin/demo"]]}}]
            }}"#,
            mock_server.uri(),
            new_cask_sha
        );
        let cask_calls = Arc::new(AtomicUsize::new(0));
        let cask_calls_clone = cask_calls.clone();
        Mock::given(method("GET"))
            .and(path("/demo.json"))
            .respond_with(move |_req: &wiremock::Request| {
                let count = cask_calls_clone.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    ResponseTemplate::new(200).set_body_string(old_cask_json.clone())
                } else {
                    ResponseTemplate::new(200).set_body_string(new_cask_json.clone())
                }
            })
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/casks/demo-old.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(old_cask_zip))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/casks/demo-new.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(new_cask_zip))
            .mount(&mock_server)
            .await;

        let mut installer = test_installer(&root, &prefix, &mock_server.uri());
        installer
            .install(&["jq".to_string(), "cask:demo".to_string()], true)
            .await
            .unwrap();
        assert!(root.join("cellar/jq/1.0.0").exists());
        assert!(root.join("cellar/cask:demo/1.0.0").exists());

        execute(&mut installer, vec![], false).await.unwrap();

        assert_eq!(installer.get_installed("jq").unwrap().version, "2.0.0");
        assert_eq!(
            installer.get_installed("cask:demo").unwrap().version,
            "2.0.0"
        );
        assert!(!root.join("cellar/jq/1.0.0").exists());
        assert!(!root.join("cellar/cask:demo/1.0.0").exists());
    }

    #[tokio::test]
    async fn upgrade_named_mixed_formula_and_cask_succeeds() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");

        let tag = get_test_bottle_tag();
        let old_bottle = create_bottle_tarball("jq", "1.0.0");
        let old_bottle_sha = sha256_hex(&old_bottle);
        let new_bottle = create_bottle_tarball("jq", "2.0.0");
        let new_bottle_sha = sha256_hex(&new_bottle);
        let old_cask_zip = create_cask_zip("bin/demo", "#!/bin/sh\necho old-demo\n");
        let old_cask_sha = sha256_hex(&old_cask_zip);
        let new_cask_zip = create_cask_zip("bin/demo", "#!/bin/sh\necho new-demo\n");
        let new_cask_sha = sha256_hex(&new_cask_zip);

        let old_formula_json = format!(
            r#"{{
                "name":"jq",
                "versions":{{"stable":"1.0.0"}},
                "dependencies":[],
                "bottle":{{"stable":{{"files":{{"{}":{{"url":"{}/bottles/jq-1.0.0.{}.bottle.tar.gz","sha256":"{}"}}}}}}}}
            }}"#,
            tag,
            mock_server.uri(),
            tag,
            old_bottle_sha
        );
        let new_formula_json = format!(
            r#"{{
                "name":"jq",
                "versions":{{"stable":"2.0.0"}},
                "dependencies":[],
                "bottle":{{"stable":{{"files":{{"{}":{{"url":"{}/bottles/jq-2.0.0.{}.bottle.tar.gz","sha256":"{}"}}}}}}}}
            }}"#,
            tag,
            mock_server.uri(),
            tag,
            new_bottle_sha
        );

        let formula_calls = Arc::new(AtomicUsize::new(0));
        let formula_calls_clone = formula_calls.clone();
        Mock::given(method("GET"))
            .and(path("/formula/jq.json"))
            .respond_with(move |_req: &wiremock::Request| {
                let count = formula_calls_clone.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    ResponseTemplate::new(200).set_body_string(old_formula_json.clone())
                } else {
                    ResponseTemplate::new(200).set_body_string(new_formula_json.clone())
                }
            })
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("/bottles/jq-1.0.0.{}.bottle.tar.gz", tag)))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(old_bottle))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/bottles/jq-2.0.0.{}.bottle.tar.gz", tag)))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(new_bottle))
            .mount(&mock_server)
            .await;

        let old_cask_json = format!(
            r#"{{
                "token":"demo",
                "version":"1.0.0",
                "url":"{}/casks/demo-old.zip",
                "sha256":"{}",
                "artifacts":[{{"binary":[["bin/demo"]]}}]
            }}"#,
            mock_server.uri(),
            old_cask_sha
        );
        let new_cask_json = format!(
            r#"{{
                "token":"demo",
                "version":"2.0.0",
                "url":"{}/casks/demo-new.zip",
                "sha256":"{}",
                "artifacts":[{{"binary":[["bin/demo"]]}}]
            }}"#,
            mock_server.uri(),
            new_cask_sha
        );
        let cask_calls = Arc::new(AtomicUsize::new(0));
        let cask_calls_clone = cask_calls.clone();
        Mock::given(method("GET"))
            .and(path("/demo.json"))
            .respond_with(move |_req: &wiremock::Request| {
                let count = cask_calls_clone.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    ResponseTemplate::new(200).set_body_string(old_cask_json.clone())
                } else {
                    ResponseTemplate::new(200).set_body_string(new_cask_json.clone())
                }
            })
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/casks/demo-old.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(old_cask_zip))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/casks/demo-new.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(new_cask_zip))
            .mount(&mock_server)
            .await;

        let mut installer = test_installer(&root, &prefix, &mock_server.uri());
        installer
            .install(&["jq".to_string(), "cask:demo".to_string()], true)
            .await
            .unwrap();

        execute(
            &mut installer,
            vec!["jq".to_string(), "cask:demo".to_string()],
            true,
        )
        .await
        .unwrap();

        assert_eq!(installer.get_installed("jq").unwrap().version, "2.0.0");
        assert_eq!(
            installer.get_installed("cask:demo").unwrap().version,
            "2.0.0"
        );
    }

    #[tokio::test]
    async fn upgrade_named_packages_continues_on_error_and_returns_non_zero() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");

        let tag = get_test_bottle_tag();
        let old_bottle = create_bottle_tarball("goodpkg", "1.0.0");
        let old_bottle_sha = sha256_hex(&old_bottle);
        let new_bottle = create_bottle_tarball("goodpkg", "2.0.0");
        let new_bottle_sha = sha256_hex(&new_bottle);

        let old_formula_json = format!(
            r#"{{
                "name":"goodpkg",
                "versions":{{"stable":"1.0.0"}},
                "dependencies":[],
                "bottle":{{"stable":{{"files":{{"{}":{{"url":"{}/bottles/goodpkg-1.0.0.{}.bottle.tar.gz","sha256":"{}"}}}}}}}}
            }}"#,
            tag,
            mock_server.uri(),
            tag,
            old_bottle_sha
        );
        let new_formula_json = format!(
            r#"{{
                "name":"goodpkg",
                "versions":{{"stable":"2.0.0"}},
                "dependencies":[],
                "bottle":{{"stable":{{"files":{{"{}":{{"url":"{}/bottles/goodpkg-2.0.0.{}.bottle.tar.gz","sha256":"{}"}}}}}}}}
            }}"#,
            tag,
            mock_server.uri(),
            tag,
            new_bottle_sha
        );
        let formula_calls = Arc::new(AtomicUsize::new(0));
        let formula_calls_clone = formula_calls.clone();
        Mock::given(method("GET"))
            .and(path("/formula/goodpkg.json"))
            .respond_with(move |_req: &wiremock::Request| {
                let count = formula_calls_clone.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    ResponseTemplate::new(200).set_body_string(old_formula_json.clone())
                } else {
                    ResponseTemplate::new(200).set_body_string(new_formula_json.clone())
                }
            })
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!(
                "/bottles/goodpkg-1.0.0.{}.bottle.tar.gz",
                tag
            )))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(old_bottle))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!(
                "/bottles/goodpkg-2.0.0.{}.bottle.tar.gz",
                tag
            )))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(new_bottle))
            .mount(&mock_server)
            .await;

        let mut installer = test_installer(&root, &prefix, &mock_server.uri());
        installer
            .install(&["goodpkg".to_string()], true)
            .await
            .unwrap();
        assert!(root.join("cellar/goodpkg/1.0.0").exists());

        let err = execute(
            &mut installer,
            vec!["goodpkg".to_string(), "missingpkg".to_string()],
            false,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, zb_core::Error::ExecutionError { .. }));
        assert_eq!(installer.get_installed("goodpkg").unwrap().version, "2.0.0");
        assert!(!root.join("cellar/goodpkg/1.0.0").exists());
    }
}
