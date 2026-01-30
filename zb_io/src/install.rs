use std::path::Path;
use std::sync::Arc;

use crate::api::ApiClient;
use crate::blob::BlobCache;
use crate::db::{Database, TapRecord};
use crate::download::{
    DownloadProgressCallback, DownloadRequest, DownloadResult, ParallelDownloader,
};
use crate::link::{LinkedFile, Linker};
use crate::materialize::Cellar;
use crate::progress::{InstallProgress, ProgressCallback};
use crate::store::Store;

use zb_core::{Error, Formula, SelectedBottle, resolve_closure, select_bottle};
use zb_core::formula::BinaryDownload;

mod install_formula;
use install_formula::{FormulaResolver, parse_formula_ref};

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
    pub artifacts: Vec<InstallArtifact>,
}

pub struct ExecuteResult {
    pub installed: usize,
}

#[derive(Clone, Debug)]
pub enum InstallArtifact {
    Bottle(SelectedBottle),
    Binary(BinaryDownload),
}

/// Internal struct for tracking processed packages during streaming install
#[derive(Clone)]
struct ProcessedPackage {
    name: String,
    version: String,
    store_key: String,
    linked_files: Vec<LinkedFile>,
}

#[cfg(test)]
mod install_tests;

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
        let root_ref = parse_formula_ref(name)?;
        // Recursively fetch all formulas we need
        let formulas = FormulaResolver::new(&self.api_client, &self.db)
            .fetch_all_formulas(name)
            .await?;

        // Resolve in topological order
        let ordered = resolve_closure(&root_ref.name, &formulas)?;

        // Build list of formulas in order
        let all_formulas: Vec<Formula> = ordered
            .iter()
            .map(|n| formulas.get(n).cloned().unwrap())
            .collect();

        // Select bottles or binary downloads for each formula
        let mut artifacts = Vec::new();
        for formula in &all_formulas {
            match select_bottle(formula) {
                Ok(bottle) => artifacts.push(InstallArtifact::Bottle(bottle)),
                Err(Error::UnsupportedBottle { .. }) => {
                    if let Some(binary) = formula.binary.clone() {
                        artifacts.push(InstallArtifact::Binary(binary));
                    } else {
                        return Err(Error::UnsupportedBottle {
                            name: formula.name.clone(),
                        });
                    }
                }
                Err(e) => return Err(e),
            }
        }

        Ok(InstallPlan {
            formulas: all_formulas,
            artifacts,
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

        // Pair formulas with install artifacts
        let to_install: Vec<(Formula, InstallArtifact)> = plan
            .formulas
            .into_iter()
            .zip(plan.artifacts.into_iter())
            .collect();

        if to_install.is_empty() {
            return Ok(ExecuteResult { installed: 0 });
        }

        // Download all artifacts
        let requests: Vec<DownloadRequest> = to_install
            .iter()
            .map(|(f, artifact)| match artifact {
                InstallArtifact::Bottle(bottle) => DownloadRequest {
                    url: bottle.url.clone(),
                    sha256: bottle.sha256.clone(),
                    name: f.name.clone(),
                },
                InstallArtifact::Binary(binary) => DownloadRequest {
                    url: binary.url.clone(),
                    sha256: binary.sha256.clone(),
                    name: f.name.clone(),
                },
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
                    let (formula, artifact) = &to_install[idx];

                    report(InstallProgress::UnpackStarted {
                        name: formula.name.clone(),
                    });

                    // Try extraction with retry logic for corrupted downloads
                    let store_entry = match artifact {
                        InstallArtifact::Bottle(bottle) => match self
                            .extract_with_retry(
                                &download,
                                formula,
                                bottle,
                                download_progress.clone(),
                            )
                            .await
                        {
                            Ok(entry) => entry,
                            Err(e) => {
                                error = Some(e);
                                continue;
                            }
                        },
                        InstallArtifact::Binary(binary) => {
                            let bin_name = binary
                                .bin
                                .as_deref()
                                .unwrap_or(&formula.name);
                            match self
                                .store
                                .ensure_binary_entry(&binary.sha256, &download.blob_path, bin_name)
                            {
                                Ok(entry) => entry,
                                Err(e) => {
                                    error = Some(e);
                                    continue;
                                }
                            }
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
                        store_key: download.sha256.clone(),
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
    pub async fn install(&mut self, name: &str, link: bool) -> Result<ExecuteResult, Error> {
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

    pub fn add_tap(&self, owner: &str, repo: &str) -> Result<bool, Error> {
        self.db.add_tap(owner, repo)
    }

    pub fn remove_tap(&self, owner: &str, repo: &str) -> Result<bool, Error> {
        self.db.remove_tap(owner, repo)
    }

    pub fn list_taps(&self) -> Result<Vec<TapRecord>, Error> {
        self.db.list_taps()
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

