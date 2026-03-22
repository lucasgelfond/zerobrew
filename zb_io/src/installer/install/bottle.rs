use std::fs;
use std::path::{Path, PathBuf};

use tracing::warn;
use zb_core::{Error, InstallMethod, formula_token};

use crate::cellar::link::Linker;
use crate::cellar::materialize::Cellar;
use crate::installer::cask::resolve_cask;
use crate::network::download::{DownloadProgressCallback, DownloadRequest, DownloadResult};
use crate::progress::InstallProgress;

use super::{Installer, MAX_CORRUPTION_RETRIES, PlannedInstall};

const CASK_APPS_DIR: &str = "Applications";

impl Installer {
    pub(super) async fn process_bottle_item(
        &mut self,
        item: &PlannedInstall,
        download: &DownloadResult,
        download_progress: &Option<DownloadProgressCallback>,
        link: bool,
        report: &impl Fn(InstallProgress),
    ) -> Result<(), Error> {
        let InstallMethod::Bottle(ref bottle) = item.method else {
            unreachable!()
        };
        let install_name = &item.install_name;
        let formula_name = &item.formula.name;
        let version = item.formula.effective_version();
        let store_key = &bottle.sha256;

        report(InstallProgress::UnpackStarted {
            name: formula_name.clone(),
        });

        let store_entry = self
            .extract_with_retry(download, &item.formula, bottle, download_progress.clone())
            .await?;

        let keg_path = self
            .cellar
            .materialize(formula_name, &version, &store_entry)?;

        report(InstallProgress::UnpackCompleted {
            name: formula_name.clone(),
        });

        let tx = self.db.transaction().inspect_err(|_| {
            Self::cleanup_materialized(&self.cellar, formula_name, &version);
        })?;

        tx.record_install(install_name, &version, store_key)
            .inspect_err(|_| {
                Self::cleanup_materialized(&self.cellar, formula_name, &version);
            })?;

        tx.commit().inspect_err(|_| {
            Self::cleanup_materialized(&self.cellar, formula_name, &version);
        })?;

        if let Err(e) = self.linker.link_opt(&keg_path) {
            warn!(formula = %install_name, error = %e, "failed to create opt link");
        }

        if link && !item.formula.is_keg_only() {
            report(InstallProgress::LinkStarted {
                name: formula_name.clone(),
            });
            match self.linker.link_keg(&keg_path) {
                Ok(linked_files) => {
                    report(InstallProgress::LinkCompleted {
                        name: formula_name.clone(),
                    });
                    self.record_linked_files(install_name, &version, &linked_files);
                }
                Err(e) => {
                    let _ = self.linker.unlink_keg(&keg_path);
                    report(InstallProgress::InstallCompleted {
                        name: formula_name.clone(),
                    });
                    return Err(e);
                }
            }
        } else if link && item.formula.is_keg_only() {
            let reason = match &item.formula.keg_only {
                zb_core::KegOnly::Reason(s) => s.clone(),
                _ if formula_name.contains('@') => "versioned formula".to_string(),
                _ => "keg-only formula".to_string(),
            };
            report(InstallProgress::LinkSkipped {
                name: formula_name.clone(),
                reason,
            });
        }

        report(InstallProgress::InstallCompleted {
            name: formula_name.clone(),
        });

        Ok(())
    }

    async fn extract_with_retry(
        &self,
        download: &DownloadResult,
        formula: &zb_core::Formula,
        bottle: &zb_core::SelectedBottle,
        progress: Option<DownloadProgressCallback>,
    ) -> Result<std::path::PathBuf, Error> {
        let mut blob_path = download.blob_path.clone();
        let mut last_error = None;

        for attempt in 0..MAX_CORRUPTION_RETRIES {
            match self.store.ensure_entry(&bottle.sha256, &blob_path) {
                Ok(entry) => return Ok(entry),
                Err(Error::StoreCorruption { message }) => {
                    self.downloader.remove_blob(&bottle.sha256);

                    if attempt + 1 < MAX_CORRUPTION_RETRIES {
                        warn!(
                            formula = %formula.name,
                            attempt = attempt + 2,
                            max_retries = MAX_CORRUPTION_RETRIES,
                            "corrupted download detected; retrying"
                        );

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

    fn record_linked_files(
        &mut self,
        name: &str,
        version: &str,
        linked_files: &[crate::cellar::link::LinkedFile],
    ) {
        if let Ok(tx) = self.db.transaction() {
            let mut ok = true;
            for linked in linked_files {
                if tx
                    .record_linked_file(
                        name,
                        version,
                        &linked.link_path.to_string_lossy(),
                        &linked.target_path.to_string_lossy(),
                    )
                    .is_err()
                {
                    ok = false;
                    break;
                }
            }
            if ok {
                let _ = tx.commit();
            }
        }
    }

    pub(super) fn cleanup_failed_install(
        linker: &Linker,
        cellar: &Cellar,
        name: &str,
        version: &str,
        keg_path: &Path,
        unlink: bool,
    ) {
        if unlink && let Err(e) = linker.unlink_keg(keg_path) {
            warn!(
                formula = %name,
                version = %version,
                error = %e,
                "failed to clean up links after install error"
            );
        }

        if unlink && let Err(e) = uninstall_cask_apps(keg_path) {
            warn!(
                formula = %name,
                version = %version,
                error = %e,
                "failed to remove installed apps after install error"
            );
        }

        if let Err(e) = cellar.remove_keg(name, version) {
            warn!(
                formula = %name,
                version = %version,
                error = %e,
                "failed to remove keg after install error"
            );
        }
    }

    pub(super) async fn install_single_cask(
        &mut self,
        token: &str,
        link: bool,
    ) -> Result<(), Error> {
        let cask_json = self.api_client.get_cask(token).await?;
        let cask = resolve_cask(token, &cask_json)?;

        let blob_path = self
            .downloader
            .download_single(
                DownloadRequest {
                    url: cask.url.clone(),
                    sha256: cask.sha256.clone(),
                    name: cask.install_name.clone(),
                },
                None,
            )
            .await?;

        let keg_path = self.cellar.keg_path(&cask.install_name, &cask.version);
        let mut cleanup = FailedInstallGuard::new(
            &self.linker,
            &self.cellar,
            &cask.install_name,
            &cask.version,
            &keg_path,
            link,
        );

        if crate::extraction::is_archive(&blob_path)? {
            let extracted = self.store.ensure_entry(&cask.sha256, &blob_path)?;
            stage_cask_artifacts(&extracted, &keg_path, &cask)?;
        } else {
            stage_raw_cask_binary(&blob_path, &keg_path, &cask)?;
        }

        let linked_files = if link {
            let mut linked_files = self.linker.link_keg(&keg_path)?;
            linked_files.extend(link_cask_apps(&keg_path, self.app_dir(), &cask)?);
            linked_files
        } else {
            Vec::new()
        };

        let tx = self.db.transaction()?;
        tx.record_install(&cask.install_name, &cask.version, &cask.sha256)?;
        for linked in &linked_files {
            tx.record_linked_file(
                &cask.install_name,
                &cask.version,
                &linked.link_path.to_string_lossy(),
                &linked.target_path.to_string_lossy(),
            )?;
        }
        tx.commit()?;

        cleanup.disarm();
        Ok(())
    }
}

pub(super) fn dependency_cellar_path(
    cellar: &Cellar,
    installed_name: &str,
    version: &str,
) -> String {
    cellar
        .keg_path(formula_token(installed_name), version)
        .display()
        .to_string()
}

struct FailedInstallGuard<'a> {
    linker: &'a Linker,
    cellar: &'a Cellar,
    name: &'a str,
    version: &'a str,
    keg_path: &'a Path,
    unlink: bool,
    armed: bool,
}

impl<'a> FailedInstallGuard<'a> {
    fn new(
        linker: &'a Linker,
        cellar: &'a Cellar,
        name: &'a str,
        version: &'a str,
        keg_path: &'a Path,
        unlink: bool,
    ) -> Self {
        Self {
            linker,
            cellar,
            name,
            version,
            keg_path,
            unlink,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for FailedInstallGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            Installer::cleanup_failed_install(
                self.linker,
                self.cellar,
                self.name,
                self.version,
                self.keg_path,
                self.unlink,
            );
        }
    }
}

fn stage_cask_artifacts(
    extracted_root: &Path,
    keg_path: &Path,
    cask: &crate::installer::cask::ResolvedCask,
) -> Result<(), Error> {
    stage_cask_apps(extracted_root, keg_path, cask)?;
    stage_cask_binaries(extracted_root, keg_path, cask)?;
    Ok(())
}

fn stage_cask_apps(
    extracted_root: &Path,
    keg_path: &Path,
    cask: &crate::installer::cask::ResolvedCask,
) -> Result<(), Error> {
    let apps_dir = keg_path.join(CASK_APPS_DIR);
    if cask.apps.is_empty() {
        return Ok(());
    }

    fs::create_dir_all(&apps_dir).map_err(Error::store("failed to create cask app dir"))?;

    for app in &cask.apps {
        let source = resolve_relative_cask_path(extracted_root, cask, &app.source, "app")?;
        if !source.exists() {
            return Err(Error::InvalidArgument {
                message: format!(
                    "cask '{}' app source '{}' not found in archive",
                    cask.token, app.source
                ),
            });
        }

        let target = apps_dir.join(&app.target);
        if target.symlink_metadata().is_ok() {
            remove_path_any(&target)
                .map_err(Error::store("failed to replace existing cask app"))?;
        }

        copy_path_recursive(&source, &target)?;
    }

    Ok(())
}

fn stage_cask_binaries(
    extracted_root: &Path,
    keg_path: &Path,
    cask: &crate::installer::cask::ResolvedCask,
) -> Result<(), Error> {
    if cask.binaries.is_empty() {
        return Ok(());
    }

    let bin_dir = keg_path.join("bin");
    fs::create_dir_all(&bin_dir).map_err(Error::store("failed to create cask bin dir"))?;

    for binary in &cask.binaries {
        let source =
            resolve_cask_binary_source_path(extracted_root, keg_path, cask, &binary.source)?;
        if !source.exists() {
            return Err(Error::InvalidArgument {
                message: format!(
                    "cask '{}' binary source '{}' not found",
                    cask.token, binary.source
                ),
            });
        }

        let target = bin_dir.join(&binary.target);
        if target.exists() {
            fs::remove_file(&target)
                .map_err(Error::store("failed to replace existing cask binary"))?;
        }

        fs::copy(&source, &target).map_err(|e| Error::StoreCorruption {
            message: format!("failed to stage cask binary '{}': {e}", binary.target),
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&target)
                .map_err(Error::store("failed to read staged cask binary metadata"))?
                .permissions();
            if perms.mode() & 0o111 == 0 {
                perms.set_mode(0o755);
                fs::set_permissions(&target, perms)
                    .map_err(Error::store("failed to make staged cask binary executable"))?;
            }
        }
    }

    Ok(())
}

fn stage_raw_cask_binary(
    blob_path: &Path,
    keg_path: &Path,
    cask: &crate::installer::cask::ResolvedCask,
) -> Result<(), Error> {
    if !cask.apps.is_empty() || cask.binaries.len() != 1 {
        return Err(Error::InvalidArgument {
            message: format!(
                "cask '{}' has unsupported raw download layout; expected exactly 1 binary artifact and no app artifacts",
                cask.token,
            ),
        });
    }

    let binary = &cask.binaries[0];
    let bin_dir = keg_path.join("bin");
    fs::create_dir_all(&bin_dir).map_err(Error::store("failed to create cask bin dir"))?;

    let target = bin_dir.join(&binary.target);
    if target.exists() {
        fs::remove_file(&target).map_err(Error::store("failed to replace existing cask binary"))?;
    }

    fs::copy(blob_path, &target).map_err(|e| Error::StoreCorruption {
        message: format!("failed to stage cask binary '{}': {e}", binary.target),
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755))
            .map_err(Error::store("failed to make staged cask binary executable"))?;
    }

    Ok(())
}

fn resolve_cask_binary_source_path(
    extracted_root: &Path,
    keg_path: &Path,
    cask: &crate::installer::cask::ResolvedCask,
    source: &str,
) -> Result<PathBuf, Error> {
    if source.starts_with("$APPDIR") {
        let relative = source
            .trim_start_matches("$APPDIR/")
            .trim_start_matches("$APPDIR");
        return resolve_relative_cask_path(&keg_path.join(CASK_APPS_DIR), cask, relative, "binary");
    }

    resolve_relative_cask_path(extracted_root, cask, source, "binary")
}

fn resolve_relative_cask_path(
    root: &Path,
    cask: &crate::installer::cask::ResolvedCask,
    source: &str,
    artifact_kind: &str,
) -> Result<PathBuf, Error> {
    let mut normalized = source.to_string();
    let caskroom_prefix = format!("$HOMEBREW_PREFIX/Caskroom/{}/{}/", cask.token, cask.version);
    if let Some(stripped) = normalized.strip_prefix(&caskroom_prefix) {
        normalized = stripped.to_string();
    }

    let source_path = Path::new(&normalized);
    if source_path.is_absolute() {
        return Err(Error::InvalidArgument {
            message: format!(
                "cask '{}' {artifact_kind} source '{}' must be a relative path",
                cask.token, source,
            ),
        });
    }

    for component in source_path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(Error::InvalidArgument {
                message: format!(
                    "cask '{}' {artifact_kind} source '{}' cannot contain '..'",
                    cask.token, source,
                ),
            });
        }
    }

    Ok(root.join(source_path))
}

fn link_cask_apps(
    keg_path: &Path,
    app_dir: &Path,
    cask: &crate::installer::cask::ResolvedCask,
) -> Result<Vec<crate::cellar::link::LinkedFile>, Error> {
    if cask.apps.is_empty() {
        return Ok(Vec::new());
    }

    fs::create_dir_all(app_dir).map_err(Error::store("failed to create app directory"))?;
    let staged_apps_dir = keg_path.join(CASK_APPS_DIR);
    let mut installed_targets = Vec::new();

    let result = (|| -> Result<(), Error> {
        for app in &cask.apps {
            let source = staged_apps_dir.join(&app.target);
            let target = app_dir.join(&app.target);

            if !source.exists() {
                return Err(Error::InvalidArgument {
                    message: format!(
                        "cask '{}' app '{}' was not staged correctly",
                        cask.token, app.target
                    ),
                });
            }

            if source.symlink_metadata().is_ok()
                && source.is_symlink()
                && target.exists()
                && let Ok(existing) = fs::read_link(&source)
            {
                let resolved = if existing.is_relative() {
                    source.parent().unwrap_or(Path::new("")).join(existing)
                } else {
                    existing
                };
                if fs::canonicalize(&resolved).ok() == fs::canonicalize(&target).ok() {
                    continue;
                }
            }

            if target.symlink_metadata().is_ok() || target.exists() {
                return Err(Error::LinkConflict {
                    conflicts: vec![zb_core::ConflictedLink {
                        path: target,
                        owned_by: None,
                    }],
                });
            }

            move_cask_app_to_target(&source, &target)?;
            installed_targets.push(target);
        }

        Ok(())
    })();

    if result.is_err() {
        for target in &installed_targets {
            let _ = remove_path_any(target);
        }
    }

    result?;

    Ok(Vec::new())
}

fn copy_path_recursive(src: &Path, dst: &Path) -> Result<(), Error> {
    let file_type =
        fs::symlink_metadata(src).map_err(Error::store("failed to read source metadata"))?;

    if file_type.file_type().is_symlink() {
        let target = fs::read_link(src).map_err(Error::store("failed to read source symlink"))?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, dst)
            .map_err(Error::store("failed to create symlink"))?;
        #[cfg(not(unix))]
        fs::copy(src, dst).map_err(Error::store("failed to copy symlink as file"))?;
        return Ok(());
    }

    if file_type.is_dir() {
        fs::create_dir_all(dst).map_err(Error::store("failed to create target directory"))?;
        for entry in fs::read_dir(src).map_err(Error::store("failed to read source directory"))? {
            let entry = entry.map_err(Error::store("failed to read source directory entry"))?;
            copy_path_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
        return Ok(());
    }

    fs::copy(src, dst).map_err(Error::store("failed to copy file"))?;
    Ok(())
}

fn move_cask_app_to_target(source: &Path, target: &Path) -> Result<(), Error> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(Error::store("failed to create app parent directory"))?;
    }

    match fs::rename(source, target) {
        Ok(()) => {}
        Err(_) => {
            copy_path_recursive(source, target)?;
            remove_path_any(source)
                .map_err(Error::store("failed to remove staged app after copy"))?;
        }
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(target, source)
        .map_err(Error::store("failed to create staged app symlink"))?;

    Ok(())
}

fn uninstall_cask_apps(keg_path: &Path) -> Result<(), Error> {
    let apps_dir = keg_path.join(CASK_APPS_DIR);
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
        if resolved.exists() {
            remove_path_any(&resolved).map_err(Error::store("failed to remove installed app"))?;
        }
    }

    Ok(())
}

pub(super) fn resolve_staged_cask_app_target(staged_path: &Path) -> Result<PathBuf, Error> {
    let target = fs::read_link(staged_path).map_err(Error::store("failed to read app symlink"))?;
    Ok(if target.is_relative() {
        staged_path.parent().unwrap_or(Path::new("")).join(target)
    } else {
        target
    })
}

pub(super) fn remove_path_any(path: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path)
    } else {
        fs::remove_dir_all(path)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use crate::cellar::Cellar;
    use crate::storage::db::Database;

    use super::*;

    #[test]
    fn dependency_cellar_path_uses_formula_token_for_tap_name() {
        let tmp = TempDir::new().unwrap();
        let cellar = Cellar::new(tmp.path()).unwrap();
        let path = dependency_cellar_path(&cellar, "hashicorp/tap/terraform", "1.10.0");

        assert!(path.ends_with("cellar/terraform/1.10.0"));
    }

    #[test]
    fn dependency_cellar_path_keeps_core_formula_name() {
        let tmp = TempDir::new().unwrap();
        let cellar = Cellar::new(tmp.path()).unwrap();
        let path = dependency_cellar_path(&cellar, "openssl@3", "3.3.2");

        assert!(path.ends_with("cellar/openssl@3/3.3.2"));
    }

    #[test]
    fn dependency_cellar_path_uses_name_from_db_record() {
        let tmp = TempDir::new().unwrap();
        let cellar = Cellar::new(tmp.path()).unwrap();

        let db_path = tmp.path().join("zb.sqlite3");
        let mut db = Database::open(&db_path).unwrap();
        let tx = db.transaction().unwrap();
        tx.record_install("hashicorp/tap/terraform", "1.10.0", "store-key")
            .unwrap();
        tx.commit().unwrap();

        let keg = db.get_installed("hashicorp/tap/terraform").unwrap();
        let path = dependency_cellar_path(&cellar, &keg.name, &keg.version);

        assert!(path.ends_with("cellar/terraform/1.10.0"));
    }

    #[test]
    fn stage_raw_cask_binary_copies_and_marks_executable() {
        let tmp = TempDir::new().unwrap();
        let blob_path = tmp.path().join("claude");
        fs::write(&blob_path, b"#!/bin/sh\necho hello").unwrap();

        let keg_path = tmp.path().join("keg");
        let cask = crate::installer::cask::ResolvedCask {
            install_name: "cask:claude-code".to_string(),
            token: "claude-code".to_string(),
            version: "1.0.0".to_string(),
            url: "https://example.com/claude".to_string(),
            sha256: "aaa".to_string(),
            binaries: vec![crate::installer::cask::CaskBinary {
                source: "claude".to_string(),
                target: "claude".to_string(),
            }],
            apps: vec![],
        };

        stage_raw_cask_binary(&blob_path, &keg_path, &cask).unwrap();

        let target = keg_path.join("bin/claude");
        assert!(target.exists());
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "#!/bin/sh\necho hello"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&target).unwrap().permissions().mode();
            assert_eq!(mode & 0o755, 0o755);
        }
    }

    #[test]
    fn stage_raw_cask_binary_rejects_multiple_binaries() {
        let tmp = TempDir::new().unwrap();
        let blob_path = tmp.path().join("blob");
        fs::write(&blob_path, b"data").unwrap();

        let keg_path = tmp.path().join("keg");
        let cask = crate::installer::cask::ResolvedCask {
            install_name: "cask:multi".to_string(),
            token: "multi".to_string(),
            version: "1.0.0".to_string(),
            url: "https://example.com/multi".to_string(),
            sha256: "bbb".to_string(),
            binaries: vec![
                crate::installer::cask::CaskBinary {
                    source: "a".to_string(),
                    target: "a".to_string(),
                },
                crate::installer::cask::CaskBinary {
                    source: "b".to_string(),
                    target: "b".to_string(),
                },
            ],
            apps: vec![],
        };

        let err = stage_raw_cask_binary(&blob_path, &keg_path, &cask).unwrap_err();
        assert!(err.to_string().contains("unsupported raw download layout"));
    }

    #[test]
    fn stage_cask_artifacts_copies_apps_and_appdir_binaries() {
        let tmp = TempDir::new().unwrap();
        let extracted_root = tmp.path().join("extract");
        let app_dir = extracted_root.join("Test.app/Contents/MacOS");
        fs::create_dir_all(&app_dir).unwrap();
        fs::write(
            extracted_root.join("Test.app/Contents/Info.plist"),
            b"plist",
        )
        .unwrap();
        fs::write(app_dir.join("test-cli"), b"#!/bin/sh\necho from-app").unwrap();

        let keg_path = tmp.path().join("keg");
        let cask = crate::installer::cask::ResolvedCask {
            install_name: "cask:test".to_string(),
            token: "test".to_string(),
            version: "1.0.0".to_string(),
            url: "https://example.com/test.zip".to_string(),
            sha256: "ccc".to_string(),
            binaries: vec![crate::installer::cask::CaskBinary {
                source: "$APPDIR/Test.app/Contents/MacOS/test-cli".to_string(),
                target: "test-cli".to_string(),
            }],
            apps: vec![crate::installer::cask::CaskApp {
                source: "Test.app".to_string(),
                target: "Test.app".to_string(),
            }],
        };

        stage_cask_artifacts(&extracted_root, &keg_path, &cask).unwrap();

        assert!(
            keg_path
                .join("Applications/Test.app/Contents/Info.plist")
                .exists()
        );
        assert_eq!(
            fs::read_to_string(keg_path.join("bin/test-cli")).unwrap(),
            "#!/bin/sh\necho from-app"
        );
    }

    #[test]
    fn link_cask_apps_creates_symlink_in_app_dir() {
        let tmp = TempDir::new().unwrap();
        let keg_path = tmp.path().join("keg");
        let staged_app = keg_path.join("Applications/Test.app");
        fs::create_dir_all(staged_app.join("Contents")).unwrap();
        fs::write(staged_app.join("Contents/Info.plist"), b"plist").unwrap();

        let app_dir = tmp.path().join("Applications");
        let cask = crate::installer::cask::ResolvedCask {
            install_name: "cask:test".to_string(),
            token: "test".to_string(),
            version: "1.0.0".to_string(),
            url: "https://example.com/test.zip".to_string(),
            sha256: "ddd".to_string(),
            binaries: vec![],
            apps: vec![crate::installer::cask::CaskApp {
                source: "Test.app".to_string(),
                target: "Test.app".to_string(),
            }],
        };

        let linked = link_cask_apps(&keg_path, &app_dir, &cask).unwrap();
        assert!(linked.is_empty());
        assert!(app_dir.join("Test.app/Contents/Info.plist").exists());
        assert_eq!(
            fs::read_link(&staged_app).unwrap(),
            app_dir.join("Test.app")
        );
    }
}
