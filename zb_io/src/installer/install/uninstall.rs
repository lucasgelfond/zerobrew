use std::fs;
use std::path::Path;

use zb_core::{Error, formula_token};

use super::Installer;
use super::bottle::{remove_path_any, resolve_staged_cask_app_target};

impl Installer {
    pub fn uninstall(&mut self, name: &str) -> Result<(), Error> {
        let installed = self.db.get_installed(name).ok_or(Error::NotInstalled {
            name: name.to_string(),
        })?;
        let keg_name = formula_token(&installed.name);

        let keg_path = self.cellar.keg_path(keg_name, &installed.version);
        self.linker.unlink_keg(&keg_path)?;
        uninstall_cask_app_targets(&keg_path)?;

        {
            let tx = self.db.transaction()?;
            tx.record_uninstall(name)?;
            tx.commit()?;
        }

        self.cellar.remove_keg(keg_name, &installed.version)?;

        Ok(())
    }

    pub fn gc(&mut self) -> Result<Vec<String>, Error> {
        let unreferenced = self.db.get_unreferenced_store_keys()?;
        let mut removed = Vec::new();

        for store_key in unreferenced {
            self.store.remove_entry(&store_key)?;
            self.db.delete_store_ref(&store_key)?;
            removed.push(store_key);
        }

        Ok(removed)
    }
}

fn uninstall_cask_app_targets(keg_path: &Path) -> Result<(), Error> {
    let apps_dir = keg_path.join("Applications");
    if !apps_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&apps_dir).map_err(Error::store("failed to read cask app dir"))? {
        let entry = entry.map_err(Error::store("failed to read cask app entry"))?;
        let staged_path = entry.path();
        if !staged_path.is_symlink() {
            continue;
        }

        let resolved = resolve_staged_cask_app_target(&staged_path)?;

        if resolved.symlink_metadata().is_ok() {
            remove_path_any(&resolved).map_err(Error::store("failed to remove installed app"))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use zip::write::SimpleFileOptions;

    use crate::cellar::Cellar;
    use crate::installer::install::test_support::*;
    use crate::network::api::ApiClient;
    use crate::storage::blob::BlobCache;
    use crate::storage::db::Database;
    use crate::storage::store::Store;
    use crate::{Installer, Linker};

    fn create_cask_app_zip() -> Vec<u8> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options = SimpleFileOptions::default().unix_permissions(0o755);
        zip.add_directory("Test.app/", options).unwrap();
        zip.add_directory("Test.app/Contents/", options).unwrap();
        zip.start_file("Test.app/Contents/Info.plist", options)
            .unwrap();
        zip.write_all(b"plist").unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[tokio::test]
    async fn uninstall_cleans_everything() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        let bottle = create_bottle_tarball("uninstallme");
        let bottle_sha = sha256_hex(&bottle);

        let tag = get_test_bottle_tag();
        let formula_json = format!(
            r#"{{
                "name": "uninstallme",
                "versions": {{ "stable": "1.0.0" }},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "{}": {{
                                "url": "{}/bottles/uninstallme-1.0.0.{}.bottle.tar.gz",
                                "sha256": "{}"
                            }}
                        }}
                    }}
                }}
            }}"#,
            tag,
            mock_server.uri(),
            tag,
            bottle_sha
        );

        Mock::given(method("GET"))
            .and(path("/formula/uninstallme.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&formula_json))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!(
                "/bottles/uninstallme-1.0.0.{}.bottle.tar.gz",
                tag
            )))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bottle.clone()))
            .mount(&mock_server)
            .await;

        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client =
            ApiClient::with_base_url(format!("{}/formula", mock_server.uri())).unwrap();
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let mut installer = Installer::new(
            api_client,
            blob_cache,
            store,
            cellar,
            linker,
            db,
            prefix.clone(),
            root.join("locks"),
        );

        installer
            .install(&["uninstallme".to_string()], true)
            .await
            .unwrap();

        assert!(installer.is_installed("uninstallme"));
        assert!(root.join("cellar/uninstallme/1.0.0").exists());
        assert!(prefix.join("bin/uninstallme").exists());

        installer.uninstall("uninstallme").unwrap();

        assert!(!installer.is_installed("uninstallme"));
        assert!(!root.join("cellar/uninstallme/1.0.0").exists());
        assert!(!prefix.join("bin/uninstallme").exists());
    }

    #[tokio::test]
    async fn gc_removes_unreferenced_store_entries() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        let bottle = create_bottle_tarball("gctest");
        let bottle_sha = sha256_hex(&bottle);

        let tag = get_test_bottle_tag();
        let formula_json = format!(
            r#"{{
                "name": "gctest",
                "versions": {{ "stable": "1.0.0" }},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "{}": {{
                                "url": "{}/bottles/gctest-1.0.0.{}.bottle.tar.gz",
                                "sha256": "{}"
                            }}
                        }}
                    }}
                }}
            }}"#,
            tag,
            mock_server.uri(),
            tag,
            bottle_sha
        );

        Mock::given(method("GET"))
            .and(path("/formula/gctest.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&formula_json))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("/bottles/gctest-1.0.0.{}.bottle.tar.gz", tag)))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bottle.clone()))
            .mount(&mock_server)
            .await;

        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client =
            ApiClient::with_base_url(format!("{}/formula", mock_server.uri())).unwrap();
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let mut installer = Installer::new(
            api_client,
            blob_cache,
            store,
            cellar,
            linker,
            db,
            prefix.clone(),
            root.join("locks"),
        );

        installer
            .install(&["gctest".to_string()], true)
            .await
            .unwrap();

        assert!(root.join("store").join(&bottle_sha).exists());

        installer.uninstall("gctest").unwrap();

        assert!(root.join("store").join(&bottle_sha).exists());

        let removed = installer.gc().unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0], bottle_sha);

        assert!(!root.join("store").join(&bottle_sha).exists());
        assert!(
            installer
                .db
                .get_unreferenced_store_keys()
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn gc_does_not_remove_referenced_store_entries() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        let bottle = create_bottle_tarball("keepme");
        let bottle_sha = sha256_hex(&bottle);

        let tag = get_test_bottle_tag();
        let formula_json = format!(
            r#"{{
                "name": "keepme",
                "versions": {{ "stable": "1.0.0" }},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "{}": {{
                                "url": "{}/bottles/keepme-1.0.0.{}.bottle.tar.gz",
                                "sha256": "{}"
                            }}
                        }}
                    }}
                }}
            }}"#,
            tag,
            mock_server.uri(),
            tag,
            bottle_sha
        );

        Mock::given(method("GET"))
            .and(path("/formula/keepme.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&formula_json))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("/bottles/keepme-1.0.0.{}.bottle.tar.gz", tag)))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bottle.clone()))
            .mount(&mock_server)
            .await;

        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client =
            ApiClient::with_base_url(format!("{}/formula", mock_server.uri())).unwrap();
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let mut installer = Installer::new(
            api_client,
            blob_cache,
            store,
            cellar,
            linker,
            db,
            prefix.clone(),
            root.join("locks"),
        );

        installer
            .install(&["keepme".to_string()], true)
            .await
            .unwrap();

        assert!(root.join("store").join(&bottle_sha).exists());

        let removed = installer.gc().unwrap();
        assert!(removed.is_empty());

        assert!(root.join("store").join(&bottle_sha).exists());
    }

    #[tokio::test]
    async fn uninstall_accepts_full_tap_reference_after_install() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        let bottle = create_bottle_tarball("terraform");
        let sha = sha256_hex(&bottle);
        let tag = get_test_bottle_tag();

        let tap_formula_rb = format!(
            r#"
class Terraform < Formula
  version "1.10.0"
  bottle do
    root_url "{}/v2/hashicorp/tap"
    sha256 {}: "{}"
  end
end
"#,
            mock_server.uri(),
            tag,
            sha
        );

        Mock::given(method("GET"))
            .and(path("/hashicorp/homebrew-tap/main/Formula/terraform.rb"))
            .respond_with(ResponseTemplate::new(200).set_body_string(tap_formula_rb))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!(
                "/v2/hashicorp/tap/terraform/blobs/sha256:{sha}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bottle))
            .mount(&mock_server)
            .await;

        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client = ApiClient::with_base_url(format!("{}/formula", mock_server.uri()))
            .unwrap()
            .with_tap_raw_base_url(mock_server.uri());
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let mut installer = Installer::new(
            api_client,
            blob_cache,
            store,
            cellar,
            linker,
            db,
            prefix.to_path_buf(),
            root.join("locks"),
        );

        installer
            .install(&["hashicorp/tap/terraform".to_string()], true)
            .await
            .unwrap();

        assert!(installer.is_installed("hashicorp/tap/terraform"));
        assert!(!installer.is_installed("terraform"));
        assert!(root.join("cellar/terraform/1.10.0").exists());
        installer.uninstall("hashicorp/tap/terraform").unwrap();
        assert!(!installer.is_installed("hashicorp/tap/terraform"));
        assert!(!root.join("cellar/terraform/1.10.0").exists());
    }

    #[tokio::test]
    async fn uninstalling_non_installed_tap_ref_does_not_remove_core_formula() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        let bottle = create_bottle_tarball("terraform");
        let sha = sha256_hex(&bottle);
        let tag = get_test_bottle_tag();
        let core_json = format!(
            r#"{{
                "name": "terraform",
                "versions": {{ "stable": "1.10.0" }},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "{}": {{
                                "url": "{}/bottles/terraform-1.10.0.{}.bottle.tar.gz",
                                "sha256": "{}"
                            }}
                        }}
                    }}
                }}
            }}"#,
            tag,
            mock_server.uri(),
            tag,
            sha
        );

        Mock::given(method("GET"))
            .and(path("/formula/terraform.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(core_json))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!(
                "/bottles/terraform-1.10.0.{}.bottle.tar.gz",
                tag
            )))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bottle))
            .mount(&mock_server)
            .await;

        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client =
            ApiClient::with_base_url(format!("{}/formula", mock_server.uri())).unwrap();
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let mut installer = Installer::new(
            api_client,
            blob_cache,
            store,
            cellar,
            linker,
            db,
            prefix.to_path_buf(),
            root.join("locks"),
        );
        installer
            .install(&["terraform".to_string()], true)
            .await
            .unwrap();
        assert!(installer.is_installed("terraform"));

        let err = installer.uninstall("hashicorp/tap/terraform").unwrap_err();
        assert!(matches!(err, zb_core::Error::NotInstalled { .. }));
        assert!(installer.is_installed("terraform"));
    }

    #[tokio::test]
    async fn uninstall_removes_installed_cask_app() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();
        let app_zip = create_cask_app_zip();
        let app_sha = sha256_hex(&app_zip);

        let cask_json = format!(
            r#"{{
                "token": "test-app",
                "version": "1.0.0",
                "url": "{}/downloads/test-app.zip",
                "sha256": "{}",
                "artifacts": [
                    {{ "app": ["Test.app"] }}
                ]
            }}"#,
            mock_server.uri(),
            app_sha
        );

        Mock::given(method("GET"))
            .and(path("/cask/test-app.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&cask_json))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/downloads/test-app.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(app_zip))
            .mount(&mock_server)
            .await;

        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        let app_dir = tmp.path().join("Applications");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client = ApiClient::with_base_url(format!("{}/formula", mock_server.uri()))
            .unwrap()
            .with_cask_base_url(format!("{}/cask", mock_server.uri()));
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let mut installer = Installer::new_with_app_dir(
            api_client,
            blob_cache,
            store,
            cellar,
            linker,
            db,
            prefix.clone(),
            app_dir.clone(),
            root.join("locks"),
        );

        installer
            .install(&["cask:test-app".to_string()], true)
            .await
            .unwrap();

        assert!(installer.is_installed("cask:test-app"));
        assert!(app_dir.join("Test.app").exists());

        installer.uninstall("cask:test-app").unwrap();

        assert!(!installer.is_installed("cask:test-app"));
        assert!(!app_dir.join("Test.app").exists());
    }
}
