use std::collections::BTreeMap;
use std::path::Path;

use crate::api::ApiClient;
use crate::blob::BlobCache;
use crate::db::Database;
use crate::download::{DownloadRequest, ParallelDownloader};
use crate::link::Linker;
use crate::materialize::Cellar;
use crate::store::Store;

use zb_core::{resolve_closure, select_bottle, Error, Formula, SelectedBottle};

pub struct Installer {
    api_client: ApiClient,
    downloader: ParallelDownloader,
    store: Store,
    cellar: Cellar,
    linker: Linker,
    db: Database,
}

pub struct InstallPlan {
    pub formulas: Vec<Formula>,
    pub bottles: Vec<SelectedBottle>,
}

impl Installer {
    pub fn new(
        api_client: ApiClient,
        blob_cache: BlobCache,
        store: Store,
        cellar: Cellar,
        linker: Linker,
        db: Database,
        download_concurrency: usize,
    ) -> Self {
        Self {
            api_client,
            downloader: ParallelDownloader::new(blob_cache, download_concurrency),
            store,
            cellar,
            linker,
            db,
        }
    }

    /// Resolve dependencies and plan the install
    pub async fn plan(&self, name: &str) -> Result<InstallPlan, Error> {
        // Recursively fetch all formulas we need
        let formulas = self.fetch_all_formulas(name).await?;

        // Resolve in topological order
        let ordered = resolve_closure(name, &formulas)?;

        // Build list of formulas in order
        let all_formulas: Vec<Formula> = ordered
            .iter()
            .map(|n| formulas.get(n).cloned().unwrap())
            .collect();

        // Select bottles for each formula
        let mut bottles = Vec::new();
        for formula in &all_formulas {
            let bottle = select_bottle(formula)?;
            bottles.push(bottle);
        }

        Ok(InstallPlan {
            formulas: all_formulas,
            bottles,
        })
    }

    /// Recursively fetch a formula and all its dependencies
    async fn fetch_all_formulas(&self, name: &str) -> Result<BTreeMap<String, Formula>, Error> {
        let mut formulas = BTreeMap::new();
        let mut to_fetch = vec![name.to_string()];

        while let Some(fetch_name) = to_fetch.pop() {
            if formulas.contains_key(&fetch_name) {
                continue;
            }

            let formula = self.api_client.get_formula(&fetch_name).await?;

            // Queue dependencies
            for dep in &formula.dependencies {
                if !formulas.contains_key(dep) {
                    to_fetch.push(dep.clone());
                }
            }

            formulas.insert(fetch_name, formula);
        }

        Ok(formulas)
    }

    /// Execute the install plan
    pub async fn execute(&mut self, plan: InstallPlan, link: bool) -> Result<(), Error> {
        // Download all bottles in parallel
        let requests: Vec<DownloadRequest> = plan
            .bottles
            .iter()
            .map(|b| DownloadRequest {
                url: b.url.clone(),
                sha256: b.sha256.clone(),
            })
            .collect();

        let blob_paths = self.downloader.download_all(requests).await?;

        // Unpack, materialize, and link each formula
        for (i, formula) in plan.formulas.iter().enumerate() {
            let blob_path = &blob_paths[i];
            let bottle = &plan.bottles[i];

            // Use sha256 as store key
            let store_key = &bottle.sha256;

            // Ensure store entry exists (unpack once)
            let store_entry = self.store.ensure_entry(store_key, blob_path)?;

            // Materialize to cellar
            let keg_path = self
                .cellar
                .materialize(&formula.name, &formula.versions.stable, &store_entry)?;

            // Link executables if requested
            let linked_files = if link {
                self.linker.link_keg(&keg_path)?
            } else {
                Vec::new()
            };

            // Record in database
            {
                let tx = self.db.transaction()?;
                tx.record_install(&formula.name, &formula.versions.stable, store_key)?;

                for linked in &linked_files {
                    tx.record_linked_file(
                        &formula.name,
                        &formula.versions.stable,
                        &linked.link_path.to_string_lossy(),
                        &linked.target_path.to_string_lossy(),
                    )?;
                }

                tx.commit()?;
            }
        }

        Ok(())
    }

    /// Convenience method to plan and execute in one call
    pub async fn install(&mut self, name: &str, link: bool) -> Result<(), Error> {
        let plan = self.plan(name).await?;
        self.execute(plan, link).await
    }

    /// Uninstall a formula
    pub fn uninstall(&mut self, name: &str) -> Result<(), Error> {
        // Check if installed
        let installed = self.db.get_installed(name).ok_or(Error::NotInstalled {
            name: name.to_string(),
        })?;

        // Unlink executables
        let keg_path = self.cellar.keg_path(name, &installed.version);
        self.linker.unlink_keg(&keg_path)?;

        // Remove from database (decrements store ref)
        {
            let tx = self.db.transaction()?;
            tx.record_uninstall(name)?;
            tx.commit()?;
        }

        // Remove cellar entry
        self.cellar.remove_keg(name, &installed.version)?;

        Ok(())
    }

    /// Garbage collect unreferenced store entries
    pub fn gc(&mut self) -> Result<Vec<String>, Error> {
        let unreferenced = self.db.get_unreferenced_store_keys()?;
        let mut removed = Vec::new();

        for store_key in unreferenced {
            self.store.remove_entry(&store_key)?;
            removed.push(store_key);
        }

        Ok(removed)
    }

    /// Check if a formula is installed
    pub fn is_installed(&self, name: &str) -> bool {
        self.db.get_installed(name).is_some()
    }

    /// Get info about an installed formula
    pub fn get_installed(&self, name: &str) -> Option<crate::db::InstalledKeg> {
        self.db.get_installed(name)
    }

    /// List all installed formulas
    pub fn list_installed(&self) -> Result<Vec<crate::db::InstalledKeg>, Error> {
        self.db.list_installed()
    }
}

/// Create an Installer with standard paths
pub fn create_installer(
    root: &Path,
    prefix: &Path,
    download_concurrency: usize,
) -> Result<Installer, Error> {
    use std::fs;

    // Ensure all directories exist
    fs::create_dir_all(root.join("db")).map_err(|e| Error::StoreCorruption {
        message: format!("failed to create db directory: {e}"),
    })?;

    let api_client = ApiClient::new();
    let blob_cache = BlobCache::new(&root.join("cache")).map_err(|e| Error::StoreCorruption {
        message: format!("failed to create blob cache: {e}"),
    })?;
    let store = Store::new(root).map_err(|e| Error::StoreCorruption {
        message: format!("failed to create store: {e}"),
    })?;
    let cellar = Cellar::new(root).map_err(|e| Error::StoreCorruption {
        message: format!("failed to create cellar: {e}"),
    })?;
    let linker = Linker::new(prefix).map_err(|e| Error::StoreCorruption {
        message: format!("failed to create linker: {e}"),
    })?;
    let db = Database::open(&root.join("db/zb.sqlite3"))?;

    Ok(Installer::new(
        api_client,
        blob_cache,
        store,
        cellar,
        linker,
        db,
        download_concurrency,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn create_bottle_tarball(formula_name: &str) -> Vec<u8> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        use tar::Builder;

        let mut builder = Builder::new(Vec::new());

        // Create bin directory with executable
        let mut header = tar::Header::new_gnu();
        header
            .set_path(format!("{}/1.0.0/bin/{}", formula_name, formula_name))
            .unwrap();
        header.set_size(20);
        header.set_mode(0o755);
        header.set_cksum();

        let content = format!("#!/bin/sh\necho {}", formula_name);
        builder.append(&header, content.as_bytes()).unwrap();

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

    #[tokio::test]
    async fn install_completes_successfully() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        // Create bottle
        let bottle = create_bottle_tarball("testpkg");
        let bottle_sha = sha256_hex(&bottle);

        // Create formula JSON
        let formula_json = format!(
            r#"{{
                "name": "testpkg",
                "versions": {{ "stable": "1.0.0" }},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "arm64_sonoma": {{
                                "url": "{}/bottles/testpkg-1.0.0.arm64_sonoma.bottle.tar.gz",
                                "sha256": "{}"
                            }}
                        }}
                    }}
                }}
            }}"#,
            mock_server.uri(),
            bottle_sha
        );

        // Mount formula API mock
        Mock::given(method("GET"))
            .and(path("/testpkg.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&formula_json))
            .mount(&mock_server)
            .await;

        // Mount bottle download mock
        Mock::given(method("GET"))
            .and(path("/bottles/testpkg-1.0.0.arm64_sonoma.bottle.tar.gz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bottle.clone()))
            .mount(&mock_server)
            .await;

        // Create installer with mocked API
        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client = ApiClient::with_base_url(mock_server.uri());
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let mut installer = Installer::new(api_client, blob_cache, store, cellar, linker, db, 4);

        // Install
        installer.install("testpkg", true).await.unwrap();

        // Verify keg exists
        assert!(root.join("cellar/testpkg/1.0.0").exists());

        // Verify link exists
        assert!(prefix.join("bin/testpkg").exists());

        // Verify database records
        let installed = installer.db.get_installed("testpkg");
        assert!(installed.is_some());
        assert_eq!(installed.unwrap().version, "1.0.0");
    }

    #[tokio::test]
    async fn uninstall_cleans_everything() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        // Create bottle
        let bottle = create_bottle_tarball("uninstallme");
        let bottle_sha = sha256_hex(&bottle);

        // Create formula JSON
        let formula_json = format!(
            r#"{{
                "name": "uninstallme",
                "versions": {{ "stable": "1.0.0" }},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "arm64_sonoma": {{
                                "url": "{}/bottles/uninstallme-1.0.0.arm64_sonoma.bottle.tar.gz",
                                "sha256": "{}"
                            }}
                        }}
                    }}
                }}
            }}"#,
            mock_server.uri(),
            bottle_sha
        );

        // Mount mocks
        Mock::given(method("GET"))
            .and(path("/uninstallme.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&formula_json))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/bottles/uninstallme-1.0.0.arm64_sonoma.bottle.tar.gz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bottle.clone()))
            .mount(&mock_server)
            .await;

        // Create installer
        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client = ApiClient::with_base_url(mock_server.uri());
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let mut installer = Installer::new(api_client, blob_cache, store, cellar, linker, db, 4);

        // Install
        installer.install("uninstallme", true).await.unwrap();

        // Verify installed
        assert!(installer.is_installed("uninstallme"));
        assert!(root.join("cellar/uninstallme/1.0.0").exists());
        assert!(prefix.join("bin/uninstallme").exists());

        // Uninstall
        installer.uninstall("uninstallme").unwrap();

        // Verify everything cleaned up
        assert!(!installer.is_installed("uninstallme"));
        assert!(!root.join("cellar/uninstallme/1.0.0").exists());
        assert!(!prefix.join("bin/uninstallme").exists());
    }

    #[tokio::test]
    async fn gc_removes_unreferenced_store_entries() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        // Create bottle
        let bottle = create_bottle_tarball("gctest");
        let bottle_sha = sha256_hex(&bottle);

        // Create formula JSON
        let formula_json = format!(
            r#"{{
                "name": "gctest",
                "versions": {{ "stable": "1.0.0" }},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "arm64_sonoma": {{
                                "url": "{}/bottles/gctest-1.0.0.arm64_sonoma.bottle.tar.gz",
                                "sha256": "{}"
                            }}
                        }}
                    }}
                }}
            }}"#,
            mock_server.uri(),
            bottle_sha
        );

        // Mount mocks
        Mock::given(method("GET"))
            .and(path("/gctest.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&formula_json))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/bottles/gctest-1.0.0.arm64_sonoma.bottle.tar.gz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bottle.clone()))
            .mount(&mock_server)
            .await;

        // Create installer
        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client = ApiClient::with_base_url(mock_server.uri());
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let mut installer = Installer::new(api_client, blob_cache, store, cellar, linker, db, 4);

        // Install and uninstall
        installer.install("gctest", true).await.unwrap();

        // Store entry should exist before GC
        assert!(root.join("store").join(&bottle_sha).exists());

        installer.uninstall("gctest").unwrap();

        // Store entry should still exist (refcount decremented but not GC'd)
        assert!(root.join("store").join(&bottle_sha).exists());

        // Run GC
        let removed = installer.gc().unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0], bottle_sha);

        // Store entry should now be gone
        assert!(!root.join("store").join(&bottle_sha).exists());
    }

    #[tokio::test]
    async fn gc_does_not_remove_referenced_store_entries() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        // Create bottle
        let bottle = create_bottle_tarball("keepme");
        let bottle_sha = sha256_hex(&bottle);

        // Create formula JSON
        let formula_json = format!(
            r#"{{
                "name": "keepme",
                "versions": {{ "stable": "1.0.0" }},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "arm64_sonoma": {{
                                "url": "{}/bottles/keepme-1.0.0.arm64_sonoma.bottle.tar.gz",
                                "sha256": "{}"
                            }}
                        }}
                    }}
                }}
            }}"#,
            mock_server.uri(),
            bottle_sha
        );

        // Mount mocks
        Mock::given(method("GET"))
            .and(path("/keepme.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&formula_json))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/bottles/keepme-1.0.0.arm64_sonoma.bottle.tar.gz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bottle.clone()))
            .mount(&mock_server)
            .await;

        // Create installer
        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client = ApiClient::with_base_url(mock_server.uri());
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let mut installer = Installer::new(api_client, blob_cache, store, cellar, linker, db, 4);

        // Install but don't uninstall
        installer.install("keepme", true).await.unwrap();

        // Store entry should exist
        assert!(root.join("store").join(&bottle_sha).exists());

        // Run GC - should not remove anything
        let removed = installer.gc().unwrap();
        assert!(removed.is_empty());

        // Store entry should still exist
        assert!(root.join("store").join(&bottle_sha).exists());
    }

    #[tokio::test]
    async fn install_with_dependencies() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        // Create bottles
        let dep_bottle = create_bottle_tarball("deplib");
        let dep_sha = sha256_hex(&dep_bottle);

        let main_bottle = create_bottle_tarball("mainpkg");
        let main_sha = sha256_hex(&main_bottle);

        // Create formula JSONs
        let dep_json = format!(
            r#"{{
                "name": "deplib",
                "versions": {{ "stable": "1.0.0" }},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "arm64_sonoma": {{
                                "url": "{}/bottles/deplib-1.0.0.arm64_sonoma.bottle.tar.gz",
                                "sha256": "{}"
                            }}
                        }}
                    }}
                }}
            }}"#,
            mock_server.uri(),
            dep_sha
        );

        let main_json = format!(
            r#"{{
                "name": "mainpkg",
                "versions": {{ "stable": "2.0.0" }},
                "dependencies": ["deplib"],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "arm64_sonoma": {{
                                "url": "{}/bottles/mainpkg-2.0.0.arm64_sonoma.bottle.tar.gz",
                                "sha256": "{}"
                            }}
                        }}
                    }}
                }}
            }}"#,
            mock_server.uri(),
            main_sha
        );

        // Mount mocks
        Mock::given(method("GET"))
            .and(path("/deplib.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&dep_json))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/mainpkg.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&main_json))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/bottles/deplib-1.0.0.arm64_sonoma.bottle.tar.gz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(dep_bottle))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/bottles/mainpkg-2.0.0.arm64_sonoma.bottle.tar.gz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(main_bottle))
            .mount(&mock_server)
            .await;

        // Create installer
        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client = ApiClient::with_base_url(mock_server.uri());
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let mut installer = Installer::new(api_client, blob_cache, store, cellar, linker, db, 4);

        // Install main package (should also install dependency)
        installer.install("mainpkg", true).await.unwrap();

        // Both packages should be installed
        assert!(installer.db.get_installed("mainpkg").is_some());
        assert!(installer.db.get_installed("deplib").is_some());
    }
}
