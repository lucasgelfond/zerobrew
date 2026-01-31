use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use crate::api::ApiClient;
use crate::blob::BlobCache;
use crate::db::Database;
use crate::download::{
    DownloadProgressCallback, DownloadRequest, DownloadResult, ParallelDownloader,
};
use crate::link::{LinkedFile, Linker};
use crate::materialize::Cellar;
use crate::progress::{InstallProgress, ProgressCallback};
use crate::store::Store;

use zb_core::{Error, Formula, SelectedBottle, resolve_closure, select_bottle};

/// Maximum number of retries for corrupted downloads
const MAX_CORRUPTION_RETRIES: usize = 3;

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

pub struct ExecuteResult {
    pub installed: usize,
}

/// Internal struct for tracking processed packages during streaming install
#[derive(Clone)]
struct ProcessedPackage {
    name: String,
    version: String,
    store_key: String,
    linked_files: Vec<LinkedFile>,
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
    pub async fn plan(&self, names: &[String]) -> Result<InstallPlan, Error> {
        // Recursively fetch all formulas we need in parallel
        let formulas = self.fetch_all_formulas(names).await?;

        // Resolve in topological order
        let ordered = resolve_closure(names, &formulas)?;

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

    /// Try to extract a download, with automatic retry on corruption
    async fn extract_with_retry(
        &self,
        download: &DownloadResult,
        formula: &Formula,
        bottle: &SelectedBottle,
        progress: Option<DownloadProgressCallback>,
    ) -> Result<std::path::PathBuf, Error> {
        let mut blob_path = download.blob_path.clone();
        let mut last_error = None;

        for attempt in 0..MAX_CORRUPTION_RETRIES {
            match self.store.ensure_entry(&bottle.sha256, &blob_path) {
                Ok(entry) => return Ok(entry),
                Err(Error::StoreCorruption { message }) => {
                    // Remove the corrupted blob
                    self.downloader.remove_blob(&bottle.sha256);

                    if attempt + 1 < MAX_CORRUPTION_RETRIES {
                        // Log retry attempt
                        eprintln!(
                            "    Corrupted download detected for {}, retrying ({}/{})...",
                            formula.name,
                            attempt + 2,
                            MAX_CORRUPTION_RETRIES
                        );

                        // Re-download
                        let request = DownloadRequest {
                            url: bottle.url.clone(),
                            sha256: bottle.sha256.clone(),
                            name: formula.name.clone(),
                        };

                        match self
                            .downloader
                            .download_single(request, progress.clone())
                            .await
                        {
                            Ok(new_path) => {
                                blob_path = new_path;
                                // Continue to next iteration to retry extraction
                            }
                            Err(e) => {
                                last_error = Some(e);
                                break;
                            }
                        }
                    } else {
                        last_error = Some(Error::StoreCorruption {
                            message: format!(
                                "{message}\n\nFailed after {MAX_CORRUPTION_RETRIES} attempts. The download may be corrupted at the source."
                            ),
                        });
                    }
                }
                Err(e) => {
                    last_error = Some(e);
                    break;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| Error::StoreCorruption {
            message: "extraction failed with unknown error".to_string(),
        }))
    }

    /// Recursively fetch all formulas and their dependencies in parallel batches
    async fn fetch_all_formulas(
        &self,
        names: &[String],
    ) -> Result<BTreeMap<String, Formula>, Error> {
        use std::collections::HashSet;

        let mut formulas = BTreeMap::new();
        let mut fetched: HashSet<String> = HashSet::new();
        // Use HashSet for deduping the fetch queue
        let mut to_fetch: HashSet<String> = names.iter().cloned().collect();

        while !to_fetch.is_empty() {
            // Fetch current batch in parallel
            let batch: Vec<String> = to_fetch.drain().filter(|n| !fetched.contains(n)).collect();

            if batch.is_empty() {
                break;
            }

            // Mark as fetched before starting (to avoid re-queueing)
            for n in &batch {
                fetched.insert(n.clone());
            }

            // Fetch all in parallel
            let futures: Vec<_> = batch
                .iter()
                .map(|n| self.api_client.get_formula(n))
                .collect();

            let results = futures::future::join_all(futures).await;

            // Process results and queue new dependencies
            for (i, result) in results.into_iter().enumerate() {
                let formula = result?;

                // Queue dependencies for next batch
                for dep in &formula.dependencies {
                    if !fetched.contains(dep) {
                        to_fetch.insert(dep.clone());
                    }
                }

                formulas.insert(batch[i].clone(), formula);
            }
        }

        Ok(formulas)
    }

    /// Execute the install plan
    pub async fn execute(&mut self, plan: InstallPlan, link: bool) -> Result<ExecuteResult, Error> {
        self.execute_with_progress(plan, link, None).await
    }

    /// Execute the install plan with progress callback
    /// Uses streaming extraction - starts extracting each package as soon as its download completes
    pub async fn execute_with_progress(
        &mut self,
        plan: InstallPlan,
        link: bool,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<ExecuteResult, Error> {
        let report = |event: InstallProgress| {
            if let Some(ref cb) = progress {
                cb(event);
            }
        };

        // Pair formulas with bottles
        let to_install: Vec<(Formula, SelectedBottle)> = plan
            .formulas
            .into_iter()
            .zip(plan.bottles.into_iter())
            .collect();

        if to_install.is_empty() {
            return Ok(ExecuteResult { installed: 0 });
        }

        // Download all bottles
        let requests: Vec<DownloadRequest> = to_install
            .iter()
            .map(|(f, b)| DownloadRequest {
                url: b.url.clone(),
                sha256: b.sha256.clone(),
                name: f.name.clone(),
            })
            .collect();

        // Convert progress callback for download
        let download_progress: Option<DownloadProgressCallback> = progress.clone().map(|cb| {
            Arc::new(move |event: InstallProgress| {
                cb(event);
            }) as DownloadProgressCallback
        });

        // Use streaming downloads - process each as it completes
        let mut rx = self
            .downloader
            .download_streaming(requests, download_progress.clone());

        // Track results by index to maintain install order for database records
        let total = to_install.len();
        let mut completed: Vec<Option<ProcessedPackage>> = vec![None; total];
        let mut error: Option<Error> = None;

        // Process downloads as they complete
        while let Some(result) = rx.recv().await {
            match result {
                Ok(download) => {
                    let idx = download.index;
                    let (formula, bottle) = &to_install[idx];

                    report(InstallProgress::UnpackStarted {
                        name: formula.name.clone(),
                    });

                    // Try extraction with retry logic for corrupted downloads
                    let store_entry = match self
                        .extract_with_retry(&download, formula, bottle, download_progress.clone())
                        .await
                    {
                        Ok(entry) => entry,
                        Err(e) => {
                            error = Some(e);
                            continue;
                        }
                    };

                    // Materialize to cellar
                    // Use effective_version() which includes rebuild suffix if applicable
                    let keg_path = match self.cellar.materialize(
                        &formula.name,
                        &formula.effective_version(),
                        &store_entry,
                    ) {
                        Ok(path) => path,
                        Err(e) => {
                            error = Some(e);
                            continue;
                        }
                    };

                    report(InstallProgress::UnpackCompleted {
                        name: formula.name.clone(),
                    });

                    // Link executables if requested
                    let linked_files = if link {
                        report(InstallProgress::LinkStarted {
                            name: formula.name.clone(),
                        });
                        match self.linker.link_keg(&keg_path) {
                            Ok(files) => {
                                report(InstallProgress::LinkCompleted {
                                    name: formula.name.clone(),
                                });
                                files
                            }
                            Err(e) => {
                                error = Some(e);
                                continue;
                            }
                        }
                    } else {
                        Vec::new()
                    };

                    // Report installation completed for this package
                    report(InstallProgress::InstallCompleted {
                        name: formula.name.clone(),
                    });

                    completed[idx] = Some(ProcessedPackage {
                        name: formula.name.clone(),
                        version: formula.effective_version(),
                        store_key: bottle.sha256.clone(),
                        linked_files,
                    });
                }
                Err(e) => {
                    error = Some(e);
                }
            }
        }

        // Return error if any download failed
        if let Some(e) = error {
            return Err(e);
        }

        // Record all successful installs in database (in order)
        for processed in completed.into_iter().flatten() {
            let tx = self.db.transaction()?;
            tx.record_install(&processed.name, &processed.version, &processed.store_key)?;

            for linked in &processed.linked_files {
                tx.record_linked_file(
                    &processed.name,
                    &processed.version,
                    &linked.link_path.to_string_lossy(),
                    &linked.target_path.to_string_lossy(),
                )?;
            }

            tx.commit()?;
        }

        Ok(ExecuteResult {
            installed: to_install.len(),
        })
    }

    /// Convenience method to plan and execute in one call
    pub async fn install(&mut self, names: &[String], link: bool) -> Result<ExecuteResult, Error> {
        let plan = self.plan(names).await?;
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

    // First ensure the root directory exists
    if !root.exists() {
        fs::create_dir_all(root).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                Error::StoreCorruption {
                    message: format!(
                        "cannot create root directory '{}': permission denied.\n\n\
                        Create it with:\n  sudo mkdir -p {} && sudo chown $USER {}",
                        root.display(),
                        root.display(),
                        root.display()
                    ),
                }
            } else {
                Error::StoreCorruption {
                    message: format!("failed to create root directory '{}': {e}", root.display()),
                }
            }
        })?;
    }

    // Ensure all subdirectories exist
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
    // Use prefix/Cellar so bottles' hardcoded rpaths work
    let cellar = Cellar::new_at(prefix.join("Cellar")).map_err(|e| Error::StoreCorruption {
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
        use flate2::Compression;
        use flate2::write::GzEncoder;
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

    fn get_test_bottle_tag() -> &'static str {
        if cfg!(target_os = "linux") {
            "x86_64_linux"
        } else {
            "arm64_sonoma"
        }
    }

    #[tokio::test]
    async fn install_completes_successfully() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        // Create bottle
        let bottle = create_bottle_tarball("testpkg");
        let bottle_sha = sha256_hex(&bottle);

        // Create formula JSON
        let tag = get_test_bottle_tag();
        let formula_json = format!(
            r#"{{
                "name": "testpkg",
                "versions": {{ "stable": "1.0.0" }},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "{}": {{
                                "url": "{}/bottles/testpkg-1.0.0.{}.bottle.tar.gz",
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

        // Mount formula API mock
        Mock::given(method("GET"))
            .and(path("/testpkg.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&formula_json))
            .mount(&mock_server)
            .await;

        // Mount bottle download mock
        Mock::given(method("GET"))
            .and(path(format!(
                "/bottles/testpkg-1.0.0.{}.bottle.tar.gz",
                tag
            )))
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
        installer
            .install(&["testpkg".to_string()], true)
            .await
            .unwrap();

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
    async fn install_multiple_roots_with_shared_dependency() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        // Create bottles
        let shared_bottle = create_bottle_tarball("sharedlib");
        let shared_sha = sha256_hex(&shared_bottle);
        let pkg1_bottle = create_bottle_tarball("pkg1");
        let pkg1_sha = sha256_hex(&pkg1_bottle);
        let pkg2_bottle = create_bottle_tarball("pkg2");
        let pkg2_sha = sha256_hex(&pkg2_bottle);

        // Formula JSONs
        let shared_json = format!(
            r#"{{"name":"sharedlib","versions":{{"stable":"1.0.0"}},"dependencies":[],"bottle":{{"stable":{{"files":{{"arm64_sonoma":{{"url":"{}/bottles/shared.tar.gz","sha256":"{}"}}}}}}}}}}"#,
            mock_server.uri(),
            shared_sha
        );
        let pkg1_json = format!(
            r#"{{"name":"pkg1","versions":{{"stable":"1.0.0"}},"dependencies":["sharedlib"],"bottle":{{"stable":{{"files":{{"arm64_sonoma":{{"url":"{}/bottles/pkg1.tar.gz","sha256":"{}"}}}}}}}}}}"#,
            mock_server.uri(),
            pkg1_sha
        );
        let pkg2_json = format!(
            r#"{{"name":"pkg2","versions":{{"stable":"1.0.0"}},"dependencies":["sharedlib"],"bottle":{{"stable":{{"files":{{"arm64_sonoma":{{"url":"{}/bottles/pkg2.tar.gz","sha256":"{}"}}}}}}}}}}"#,
            mock_server.uri(),
            pkg2_sha
        );

        // Mount all mocks
        for (name, json) in [
            ("sharedlib", &shared_json),
            ("pkg1", &pkg1_json),
            ("pkg2", &pkg2_json),
        ] {
            Mock::given(method("GET"))
                .and(path(format!("/{}.json", name)))
                .respond_with(ResponseTemplate::new(200).set_body_string(json))
                .mount(&mock_server)
                .await;
        }
        for (name, bottle) in [
            ("shared", &shared_bottle),
            ("pkg1", &pkg1_bottle),
            ("pkg2", &pkg2_bottle),
        ] {
            Mock::given(method("GET"))
                .and(path(format!("/bottles/{}.tar.gz", name)))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(bottle.clone()))
                .mount(&mock_server)
                .await;
        }

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

        // Verify that installing multiple roots works and shared dependencies (sharedlib) are only installed once
        installer
            .install(&["pkg1".to_string(), "pkg2".to_string()], true)
            .await
            .unwrap();

        // Verify all 3 packages are installed
        assert!(installer.db.get_installed("pkg1").is_some());
        assert!(installer.db.get_installed("pkg2").is_some());
        assert!(installer.db.get_installed("sharedlib").is_some());

        let installed = installer.list_installed().unwrap();
        assert_eq!(installed.len(), 3);
    }
}
