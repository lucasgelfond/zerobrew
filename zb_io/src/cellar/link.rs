use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use zb_core::{ConflictedLink, Error};

/// Keg directories linked into the prefix. Like Homebrew, `libexec` is not
/// linked: it holds files private to a keg, like the Python formulae's
/// unversioned `python` and `pip`, which would collide across versions.
const LINK_DIRS: &[&str] = &["bin", "sbin", "lib", "include", "share", "etc"];

/// Directories older versions of zerobrew linked. Still unlinked, so those
/// links are cleaned up on uninstall and upgrade.
const LEGACY_LINK_DIRS: &[&str] = &["libexec"];

pub struct Linker {
    prefix: PathBuf,
    bin_dir: PathBuf,
    opt_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct LinkedFile {
    pub link_path: PathBuf,
    pub target_path: PathBuf,
}

fn keg_name_from_path(path: &Path) -> Option<String> {
    let components: Vec<_> = path.components().collect();
    for (i, c) in components.iter().enumerate() {
        if let Component::Normal(s) = c
            && s.eq_ignore_ascii_case("cellar")
            && let Some(Component::Normal(name)) = components.get(i + 1)
        {
            return name.to_str().map(String::from);
        }
    }
    None
}

/// Strip `.` and resolve `..` components without touching the filesystem, so
/// dangling symlink targets can still be attributed to a keg.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Read the symlink at `dst` and resolve a relative target against its parent
/// directory. Returns `None` when `dst` is not a symlink.
fn resolve_link_target(dst: &Path) -> Option<PathBuf> {
    let target = fs::read_link(dst).ok()?;
    Some(if target.is_relative() {
        dst.parent().unwrap_or(Path::new("")).join(&target)
    } else {
        target
    })
}

fn keg_name_from_symlink(dst: &Path) -> Option<String> {
    let resolved = resolve_link_target(dst)?;
    match fs::canonicalize(&resolved) {
        Ok(canonical) => keg_name_from_path(&canonical),
        // Dangling link (e.g. the keg it pointed into was removed): the
        // target path still identifies the owning keg.
        Err(_) => keg_name_from_path(&normalize_lexically(&resolved)),
    }
}

/// Whether the symlink at `dst` may be silently replaced by a link to `src`.
///
/// Regression guard for #331 (https://github.com/lucasgelfond/zerobrew/issues/331):
/// upgrades and reinstalls used to report the previous version's symlinks as
/// conflicts "belonging to" the formula itself, leaving the prefix pointing at
/// the old keg. A link is replaceable when it belongs to another version of
/// the same keg, or when its target no longer exists (dead links block fresh
/// installs but protect nothing).
fn can_replace_existing_link(src: &Path, dst: &Path) -> bool {
    let Some(resolved) = resolve_link_target(dst) else {
        return false;
    };
    if !resolved.exists() {
        return true;
    }
    match (keg_name_from_symlink(dst), keg_name_from_path(src)) {
        (Some(old_owner), Some(new_owner)) => old_owner == new_owner,
        _ => false,
    }
}

impl Linker {
    pub fn new(prefix: &Path) -> io::Result<Self> {
        let bin_dir = prefix.join("bin");
        let opt_dir = prefix.join("opt");
        fs::create_dir_all(&bin_dir)?;
        fs::create_dir_all(&opt_dir)?;

        for dir in LINK_DIRS {
            if *dir != "bin" {
                fs::create_dir_all(prefix.join(dir))?;
            }
        }

        Ok(Self {
            prefix: prefix.to_path_buf(),
            bin_dir,
            opt_dir,
        })
    }

    /// Pre-flight check: scan all destinations for conflicts without creating any symlinks.
    /// Returns Ok(()) if no conflicts, or Err(LinkConflict) with all conflicts collected.
    pub fn check_conflicts(&self, keg_path: &Path) -> Result<(), Error> {
        let mut conflicts = Vec::new();
        for dir_name in LINK_DIRS {
            let src_dir = keg_path.join(dir_name);
            let dst_dir = self.prefix.join(dir_name);
            if src_dir.exists() {
                Self::collect_conflicts(&src_dir, &dst_dir, &mut conflicts);
            }
        }
        if conflicts.is_empty() {
            Ok(())
        } else {
            Err(Error::LinkConflict { conflicts })
        }
    }

    fn collect_conflicts(src: &Path, dst: &Path, conflicts: &mut Vec<ConflictedLink>) {
        let entries = match fs::read_dir(src) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let file_name = entry.file_name();

            let src_path = entry.path();
            let dst_path = dst.join(&file_name);

            // Use src_path.is_dir() which follows symlinks, so that keg entries
            // like `man -> ../gnuman` (symlinks to directories) are treated as dirs.
            if src_path.is_dir() {
                // When the destination is a symlink to a directory, actual linking will
                // expand it into individual file symlinks. Check the expanded contents.
                if dst_path.symlink_metadata().is_ok()
                    && dst_path.is_symlink()
                    && let Ok(old_target) = fs::read_link(&dst_path)
                {
                    let resolved = if old_target.is_relative() {
                        dst_path.parent().unwrap_or(Path::new("")).join(&old_target)
                    } else {
                        old_target
                    };
                    Self::collect_conflicts_merged(&src_path, &resolved, &dst_path, conflicts);
                    continue;
                }
                Self::collect_conflicts(&src_path, &dst_path, conflicts);
                continue;
            }

            if dst_path.symlink_metadata().is_ok() {
                if let Ok(target) = fs::read_link(&dst_path) {
                    let resolved = if target.is_relative() {
                        dst_path.parent().unwrap_or(Path::new("")).join(&target)
                    } else {
                        target
                    };
                    if fs::canonicalize(&resolved).ok() == fs::canonicalize(&src_path).ok() {
                        continue;
                    }
                    if can_replace_existing_link(&src_path, &dst_path) {
                        continue;
                    }
                }
                conflicts.push(ConflictedLink {
                    path: dst_path.clone(),
                    owned_by: keg_name_from_symlink(&dst_path),
                });
            } else if dst_path.exists() {
                conflicts.push(ConflictedLink {
                    path: dst_path,
                    owned_by: None,
                });
            }
        }
    }

    /// Check for conflicts when a directory symlink will be expanded into file-level links.
    /// `src` is the new keg's directory, `old_target` is where the existing symlink points,
    /// and `dst` is the prefix directory that will be created.
    fn collect_conflicts_merged(
        src: &Path,
        old_target: &Path,
        dst: &Path,
        conflicts: &mut Vec<ConflictedLink>,
    ) {
        let new_entries = match fs::read_dir(src) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in new_entries.flatten() {
            let file_name = entry.file_name();

            let src_path = entry.path();
            let matching_old = old_target.join(&file_name);
            let dst_path = dst.join(&file_name);

            if src_path.is_dir() {
                if matching_old.exists() {
                    Self::collect_conflicts_merged(&src_path, &matching_old, &dst_path, conflicts);
                } else {
                    Self::collect_conflicts(&src_path, &dst_path, conflicts);
                }
                continue;
            }

            if matching_old.exists()
                && fs::canonicalize(&matching_old).ok() != fs::canonicalize(&src_path).ok()
            {
                // Another version of the same keg (upgrade through a legacy
                // whole-directory symlink) will be replaced during linking.
                let old_owner = fs::canonicalize(&matching_old)
                    .ok()
                    .and_then(|p| keg_name_from_path(&p));
                if old_owner.is_some() && old_owner == keg_name_from_path(&src_path) {
                    continue;
                }
                conflicts.push(ConflictedLink {
                    path: dst_path,
                    owned_by: keg_name_from_symlink(dst).or_else(|| keg_name_from_path(old_target)),
                });
            }
        }
    }

    pub fn link_keg(&self, keg_path: &Path) -> Result<Vec<LinkedFile>, Error> {
        self.check_conflicts(keg_path)?;
        self.link_opt(keg_path)?;
        let mut linked = Vec::new();
        for dir_name in LINK_DIRS {
            let src_dir = keg_path.join(dir_name);
            let dst_dir = self.prefix.join(dir_name);
            if src_dir.exists() {
                linked.extend(Self::link_recursive(&src_dir, &dst_dir)?);
            }
        }
        Ok(linked)
    }

    fn link_recursive(src: &Path, dst: &Path) -> Result<Vec<LinkedFile>, Error> {
        let mut linked = Vec::new();
        if !dst.exists() {
            fs::create_dir_all(dst).map_err(Error::store("failed to create directory"))?;
        }

        for entry in fs::read_dir(src).map_err(Error::store("failed to read directory"))? {
            let entry = entry.map_err(Error::store("failed to read directory entry"))?;
            let file_name = entry.file_name();

            let src_path = entry.path();
            let dst_path = dst.join(&file_name);

            // Use src_path.is_dir() which follows symlinks, so that keg entries
            // like `man -> ../gnuman` (symlinks to directories) are expanded
            // into individual file symlinks instead of conflicting.
            if src_path.is_dir() {
                if dst_path.symlink_metadata().is_ok() && dst_path.is_symlink() {
                    let target = fs::read_link(&dst_path)
                        .map_err(Error::store("failed to read symlink target"))?;
                    let old_target = if target.is_relative() {
                        dst_path.parent().unwrap_or(Path::new("")).join(&target)
                    } else {
                        target
                    };
                    let _ = fs::remove_file(&dst_path);
                    // A dangling directory symlink (e.g. the old keg was
                    // removed) has nothing left to expand.
                    if old_target.exists() {
                        Self::link_recursive(&old_target, &dst_path)?;
                    }
                }
                linked.extend(Self::link_recursive(&src_path, &dst_path)?);
                continue;
            }

            if dst_path.symlink_metadata().is_ok() {
                if let Ok(target) = fs::read_link(&dst_path) {
                    let resolved = if target.is_relative() {
                        dst_path.parent().unwrap_or(Path::new("")).join(&target)
                    } else {
                        target
                    };
                    if fs::canonicalize(&resolved).ok() == fs::canonicalize(&src_path).ok() {
                        if resolved.exists() {
                            linked.push(LinkedFile {
                                link_path: dst_path,
                                target_path: src_path,
                            });
                            continue;
                        } else {
                            let _ = fs::remove_file(&dst_path);
                        }
                    } else if can_replace_existing_link(&src_path, &dst_path) {
                        let _ = fs::remove_file(&dst_path);
                    } else {
                        return Err(Error::LinkConflict {
                            conflicts: vec![ConflictedLink {
                                path: dst_path.clone(),
                                owned_by: keg_name_from_symlink(&dst_path),
                            }],
                        });
                    }
                } else {
                    return Err(Error::LinkConflict {
                        conflicts: vec![ConflictedLink {
                            path: dst_path,
                            owned_by: None,
                        }],
                    });
                }
            } else if dst_path.exists() {
                return Err(Error::LinkConflict {
                    conflicts: vec![ConflictedLink {
                        path: dst_path,
                        owned_by: None,
                    }],
                });
            }

            #[cfg(unix)]
            std::os::unix::fs::symlink(&src_path, &dst_path)
                .map_err(Error::store("failed to create symlink"))?;
            linked.push(LinkedFile {
                link_path: dst_path,
                target_path: src_path,
            });
        }
        Ok(linked)
    }

    pub fn unlink_keg(&self, keg_path: &Path) -> Result<Vec<PathBuf>, Error> {
        self.unlink_opt(keg_path)?;
        let mut unlinked = Vec::new();
        for dir_name in LINK_DIRS.iter().chain(LEGACY_LINK_DIRS) {
            let src_dir = keg_path.join(dir_name);
            let dst_dir = self.prefix.join(dir_name);
            if src_dir.exists() {
                unlinked.extend(Self::unlink_recursive(&src_dir, &dst_dir)?);
            }
        }
        Ok(unlinked)
    }

    pub fn collect_linked_files(&self, keg_path: &Path) -> Result<Vec<LinkedFile>, Error> {
        let mut linked = Vec::new();
        for dir_name in LINK_DIRS.iter().chain(LEGACY_LINK_DIRS) {
            let src_dir = keg_path.join(dir_name);
            let dst_dir = self.prefix.join(dir_name);
            if src_dir.exists() {
                linked.extend(Self::collect_linked_recursive(&src_dir, &dst_dir)?);
            }
        }
        Ok(linked)
    }

    fn unlink_recursive(src: &Path, dst: &Path) -> Result<Vec<PathBuf>, Error> {
        let mut unlinked = Vec::new();
        if !src.exists() || !dst.exists() {
            return Ok(unlinked);
        }
        for entry in fs::read_dir(src).map_err(Error::store("failed to read directory"))? {
            let entry = entry.map_err(Error::store("failed to read directory entry"))?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if src_path.is_dir() && dst_path.is_dir() && !dst_path.is_symlink() {
                unlinked.extend(Self::unlink_recursive(&src_path, &dst_path)?);
                if let Ok(mut entries) = fs::read_dir(&dst_path)
                    && entries.next().is_none()
                {
                    let _ = fs::remove_dir(&dst_path);
                }
                continue;
            }

            if let Ok(target) = fs::read_link(&dst_path) {
                let resolved = if target.is_relative() {
                    dst_path.parent().unwrap_or(Path::new("")).join(&target)
                } else {
                    target
                };
                if fs::canonicalize(&resolved).ok() == fs::canonicalize(&src_path).ok() {
                    let _ = fs::remove_file(&dst_path);
                    unlinked.push(dst_path);
                }
            }
        }
        Ok(unlinked)
    }

    fn collect_linked_recursive(src: &Path, dst: &Path) -> Result<Vec<LinkedFile>, Error> {
        let mut linked = Vec::new();
        if !src.exists() || !dst.exists() {
            return Ok(linked);
        }
        for entry in fs::read_dir(src).map_err(Error::store("failed to read directory"))? {
            let entry = entry.map_err(Error::store("failed to read directory entry"))?;
            let file_name = entry.file_name();

            let src_path = entry.path();
            let dst_path = dst.join(file_name);

            if src_path.is_dir() && dst_path.is_dir() && !dst_path.is_symlink() {
                linked.extend(Self::collect_linked_recursive(&src_path, &dst_path)?);
                continue;
            }

            if let Ok(target) = fs::read_link(&dst_path) {
                let resolved = if target.is_relative() {
                    dst_path.parent().unwrap_or(Path::new("")).join(&target)
                } else {
                    target
                };
                if fs::canonicalize(&resolved).ok() == fs::canonicalize(&src_path).ok() {
                    linked.push(LinkedFile {
                        link_path: dst_path,
                        target_path: src_path,
                    });
                }
            }
        }
        Ok(linked)
    }

    fn unlink_opt(&self, keg_path: &Path) -> Result<(), Error> {
        let name = keg_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str());
        if let Some(name) = name {
            let opt_link = self.opt_dir.join(name);
            if let Ok(target) = fs::read_link(&opt_link) {
                let resolved = if target.is_relative() {
                    opt_link.parent().unwrap_or(Path::new("")).join(&target)
                } else {
                    target
                };
                if fs::canonicalize(&resolved).ok() == fs::canonicalize(keg_path).ok() {
                    let _ = fs::remove_file(&opt_link);
                }
            }
        }
        Ok(())
    }

    pub fn link_opt(&self, keg_path: &Path) -> Result<(), Error> {
        let name = keg_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .ok_or_else(|| Error::StoreCorruption {
                message: "invalid keg path".into(),
            })?;
        let opt_link = self.opt_dir.join(name);
        if opt_link.symlink_metadata().is_ok() {
            if let Ok(target) = fs::read_link(&opt_link) {
                let resolved = if target.is_relative() {
                    opt_link.parent().unwrap_or(Path::new("")).join(&target)
                } else {
                    target
                };
                if fs::canonicalize(&resolved).ok() == fs::canonicalize(keg_path).ok() {
                    return Ok(());
                }
            }
            let _ = fs::remove_file(&opt_link);
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(keg_path, &opt_link)
            .map_err(Error::store("failed to create opt symlink"))?;
        Ok(())
    }

    pub fn is_linked(&self, keg_path: &Path) -> bool {
        let keg_bin = keg_path.join("bin");
        if !keg_bin.exists() {
            return false;
        }
        if let Ok(entries) = fs::read_dir(&keg_bin) {
            for entry in entries.flatten() {
                let dst_path = self.bin_dir.join(entry.file_name());
                if let Ok(target) = fs::read_link(&dst_path) {
                    let resolved = if target.is_relative() {
                        dst_path.parent().unwrap_or(Path::new("")).join(&target)
                    } else {
                        target
                    };
                    if fs::canonicalize(&resolved).ok() == fs::canonicalize(entry.path()).ok() {
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn setup_keg(tmp: &TempDir, name: &str) -> PathBuf {
        let keg_path = tmp.path().join("cellar").join(name).join("1.0.0");
        let bin_dir = keg_path.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let exe = bin_dir.join(name);
        fs::write(&exe, b"hi").unwrap();
        fs::set_permissions(&exe, PermissionsExt::from_mode(0o755)).unwrap();
        keg_path
    }

    #[test]
    fn links_executables_to_bin() {
        let tmp = TempDir::new().unwrap();
        let keg = setup_keg(&tmp, "foo");
        let linker = Linker::new(tmp.path()).unwrap();
        linker.link_keg(&keg).unwrap();
        assert!(tmp.path().join("bin/foo").exists());
    }

    #[test]
    fn links_sbin() {
        let tmp = TempDir::new().unwrap();
        let keg = tmp.path().join("cellar/php/8.5.11");
        fs::create_dir_all(keg.join("sbin")).unwrap();
        fs::write(keg.join("sbin/php-fpm"), b"php-fpm").unwrap();

        let linker = Linker::new(tmp.path()).unwrap();
        linker.link_keg(&keg).unwrap();

        assert!(tmp.path().join("sbin/php-fpm").is_symlink());
    }

    #[test]
    fn merging_directories_works() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let linker = Linker::new(prefix).unwrap();
        let keg1 = prefix.join("cellar/pkg1/1.0.0");
        fs::create_dir_all(keg1.join("lib/pkgconfig")).unwrap();
        fs::write(keg1.join("lib/pkgconfig/pkg1.pc"), b"").unwrap();
        let keg2 = prefix.join("cellar/pkg2/1.0.0");
        fs::create_dir_all(keg2.join("lib/pkgconfig")).unwrap();
        fs::write(keg2.join("lib/pkgconfig/pkg2.pc"), b"").unwrap();
        linker.link_keg(&keg1).unwrap();
        linker.link_keg(&keg2).unwrap();
        assert!(prefix.join("lib/pkgconfig/pkg1.pc").exists());
        assert!(prefix.join("lib/pkgconfig/pkg2.pc").exists());
    }

    #[test]
    fn does_not_link_libexec() {
        // python@3.13 and python@3.14 both ship libexec/bin/python; linking
        // libexec made the second one conflict with the first.
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let linker = Linker::new(prefix).unwrap();

        for keg in ["Cellar/python@3.13/3.13.15", "Cellar/python@3.14/3.14.7"] {
            let keg = prefix.join(keg);
            fs::create_dir_all(keg.join("libexec/bin")).unwrap();
            fs::write(keg.join("libexec/bin/python"), b"python").unwrap();
            linker.link_keg(&keg).unwrap();
        }

        assert!(!prefix.join("libexec").exists());
    }

    #[test]
    fn unlink_removes_links_into_legacy_libexec() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let linker = Linker::new(prefix).unwrap();
        let keg = prefix.join("Cellar/git/2.52.0");
        fs::create_dir_all(keg.join("libexec/git-core")).unwrap();
        fs::write(keg.join("libexec/git-core/git-remote-https"), b"helper").unwrap();

        // A link left behind by a version of zerobrew that linked libexec.
        let legacy_link = prefix.join("libexec/git-core/git-remote-https");
        fs::create_dir_all(legacy_link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(keg.join("libexec/git-core/git-remote-https"), &legacy_link)
            .unwrap();

        linker.unlink_keg(&keg).unwrap();

        assert!(legacy_link.symlink_metadata().is_err());
    }

    #[test]
    fn check_conflicts_passes_when_clean() {
        let tmp = TempDir::new().unwrap();
        let keg = setup_keg(&tmp, "foo");
        let linker = Linker::new(tmp.path()).unwrap();
        assert!(linker.check_conflicts(&keg).is_ok());
    }

    #[test]
    fn check_conflicts_detects_conflicting_file() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let linker = Linker::new(prefix).unwrap();

        let keg1 = setup_keg(&tmp, "pkg1");
        linker.link_keg(&keg1).unwrap();

        // Create a second keg with a conflicting binary name
        let keg2 = prefix.join("cellar/pkg2/1.0.0");
        let bin2 = keg2.join("bin");
        fs::create_dir_all(&bin2).unwrap();
        fs::write(bin2.join("pkg1"), b"conflict").unwrap();
        fs::set_permissions(bin2.join("pkg1"), PermissionsExt::from_mode(0o755)).unwrap();

        let result = linker.check_conflicts(&keg2);
        assert!(result.is_err());
        if let Err(Error::LinkConflict { conflicts }) = result {
            assert_eq!(conflicts.len(), 1);
            assert!(conflicts[0].path.ends_with("bin/pkg1"));
            assert_eq!(conflicts[0].owned_by.as_deref(), Some("pkg1"));
        }
    }

    #[test]
    fn check_conflicts_collects_all_conflicts() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let linker = Linker::new(prefix).unwrap();

        // Create keg1 with two binaries
        let keg1 = prefix.join("Cellar/pkg1/1.0.0");
        let bin1 = keg1.join("bin");
        fs::create_dir_all(&bin1).unwrap();
        fs::write(bin1.join("tool-a"), b"a").unwrap();
        fs::write(bin1.join("tool-b"), b"b").unwrap();
        linker.link_keg(&keg1).unwrap();

        // Create keg2 with overlapping binaries
        let keg2 = prefix.join("Cellar/pkg2/1.0.0");
        let bin2 = keg2.join("bin");
        fs::create_dir_all(&bin2).unwrap();
        fs::write(bin2.join("tool-a"), b"x").unwrap();
        fs::write(bin2.join("tool-b"), b"y").unwrap();

        let result = linker.check_conflicts(&keg2);
        assert!(result.is_err());
        if let Err(Error::LinkConflict { conflicts }) = result {
            assert_eq!(conflicts.len(), 2);
        }
    }

    #[test]
    fn link_keg_rejects_conflicts_without_creating_links() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let linker = Linker::new(prefix).unwrap();

        let keg1 = setup_keg(&tmp, "alpha");
        linker.link_keg(&keg1).unwrap();

        // keg2 has a binary named "alpha" that conflicts
        let keg2 = prefix.join("cellar/beta/1.0.0");
        let bin2 = keg2.join("bin");
        fs::create_dir_all(&bin2).unwrap();
        fs::write(bin2.join("alpha"), b"other").unwrap();
        fs::write(bin2.join("beta-only"), b"unique").unwrap();

        assert!(linker.link_keg(&keg2).is_err());
        // The non-conflicting file should NOT have been linked (all-or-none)
        assert!(!prefix.join("bin/beta-only").exists());
        // The opt link should also not exist
        assert!(!prefix.join("opt/beta").exists());
    }

    #[test]
    fn symlink_to_directory_in_keg_expands_without_conflict() {
        // Reproduces the gnu-sed / gnu-tar / findutils conflict from issue #69:
        // https://github.com/lucasgelfond/zerobrew/issues/69
        // each keg has `share/gnubin/man -> ../gnuman` (symlink to directory).
        // The linker should expand these into individual file symlinks so that
        // man pages from different kegs coexist.
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let linker = Linker::new(prefix).unwrap();

        // keg1: share/gnubin/man -> ../gnuman, with gnuman/man1/sed.1
        let keg1 = prefix.join("Cellar/gnu-sed/4.9");
        fs::create_dir_all(keg1.join("share/gnuman/man1")).unwrap();
        fs::write(keg1.join("share/gnuman/man1/sed.1"), b"sed man").unwrap();
        fs::create_dir_all(keg1.join("share/gnubin")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("../gnuman", keg1.join("share/gnubin/man")).unwrap();

        // keg2: share/gnubin/man -> ../gnuman, with gnuman/man1/tar.1
        let keg2 = prefix.join("Cellar/gnu-tar/1.35");
        fs::create_dir_all(keg2.join("share/gnuman/man1")).unwrap();
        fs::write(keg2.join("share/gnuman/man1/tar.1"), b"tar man").unwrap();
        fs::create_dir_all(keg2.join("share/gnubin")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("../gnuman", keg2.join("share/gnubin/man")).unwrap();

        // Both should link without conflicts
        linker.link_keg(&keg1).unwrap();
        linker.link_keg(&keg2).unwrap();

        // Both man pages should be accessible
        assert!(prefix.join("share/gnubin/man/man1/sed.1").exists());
        assert!(prefix.join("share/gnubin/man/man1/tar.1").exists());
        // gnuman dirs should also be expanded and merged
        assert!(prefix.join("share/gnuman/man1/sed.1").exists());
        assert!(prefix.join("share/gnuman/man1/tar.1").exists());
    }

    #[test]
    fn check_conflicts_passes_for_symlink_to_directory() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let linker = Linker::new(prefix).unwrap();

        let keg1 = prefix.join("Cellar/pkg1/1.0.0");
        fs::create_dir_all(keg1.join("libexec/realdir")).unwrap();
        fs::write(keg1.join("libexec/realdir/file1"), b"a").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("realdir", keg1.join("libexec/alias")).unwrap();

        let keg2 = prefix.join("Cellar/pkg2/1.0.0");
        fs::create_dir_all(keg2.join("libexec/realdir")).unwrap();
        fs::write(keg2.join("libexec/realdir/file2"), b"b").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("realdir", keg2.join("libexec/alias")).unwrap();

        linker.link_keg(&keg1).unwrap();
        // Pre-flight check should pass since the files don't overlap
        assert!(linker.check_conflicts(&keg2).is_ok());
    }

    #[test]
    fn upgrade_relinks_same_formula_to_new_version() {
        // Regression test for #331 (https://github.com/lucasgelfond/zerobrew/issues/331):
        // installing a newer version of an already-linked formula reported the
        // old version's symlinks as conflicts "belonging to" the formula
        // itself, so the prefix kept pointing at the old keg forever.
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let linker = Linker::new(prefix).unwrap();

        let old_keg = prefix.join("cellar/gh/1.0.0");
        fs::create_dir_all(old_keg.join("bin")).unwrap();
        fs::create_dir_all(old_keg.join("share/man/man1")).unwrap();
        fs::write(old_keg.join("bin/gh"), b"old").unwrap();
        fs::write(old_keg.join("share/man/man1/gh.1"), b"old man").unwrap();
        linker.link_keg(&old_keg).unwrap();

        let new_keg = prefix.join("cellar/gh/2.0.0");
        fs::create_dir_all(new_keg.join("bin")).unwrap();
        fs::create_dir_all(new_keg.join("share/man/man1")).unwrap();
        fs::write(new_keg.join("bin/gh"), b"new").unwrap();
        fs::write(new_keg.join("share/man/man1/gh.1"), b"new man").unwrap();

        assert!(
            linker.check_conflicts(&new_keg).is_ok(),
            "another version of the same formula must not count as a conflict"
        );
        linker.link_keg(&new_keg).unwrap();

        for link in ["bin/gh", "share/man/man1/gh.1"] {
            let target = fs::read_link(prefix.join(link)).unwrap();
            let target = target.to_string_lossy();
            assert!(
                target.contains("2.0.0"),
                "{link} must point at 2.0.0, got {target}"
            );
        }
    }

    #[test]
    fn relinks_when_old_keg_directory_was_removed() {
        // #331 fallout: an upgrade that removed the old keg but died before
        // relinking leaves dangling same-formula symlinks; the next install
        // must replace them instead of conflicting.
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let linker = Linker::new(prefix).unwrap();

        let old_keg = setup_keg(&tmp, "foo");
        linker.link_keg(&old_keg).unwrap();
        fs::remove_dir_all(&old_keg).unwrap();
        assert!(prefix.join("bin/foo").is_symlink());

        let new_keg = prefix.join("cellar/foo/2.0.0");
        fs::create_dir_all(new_keg.join("bin")).unwrap();
        fs::write(new_keg.join("bin/foo"), b"new").unwrap();

        assert!(linker.check_conflicts(&new_keg).is_ok());
        linker.link_keg(&new_keg).unwrap();

        let target = fs::read_link(prefix.join("bin/foo")).unwrap();
        assert!(target.to_string_lossy().contains("2.0.0"));
        assert!(prefix.join("bin/foo").exists(), "link must not be dangling");
    }

    #[test]
    fn replaces_dangling_symlink_from_other_formula() {
        // Orphaned links whose keg no longer exists (#188 fallout) used to
        // block unrelated installs with phantom conflicts. A dead link
        // protects nothing and is safe to replace.
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let linker = Linker::new(prefix).unwrap();

        std::os::unix::fs::symlink(
            prefix.join("cellar/ghost/1.0.0/bin/tool"),
            prefix.join("bin/tool"),
        )
        .unwrap();

        let keg = prefix.join("cellar/tool/1.0.0");
        fs::create_dir_all(keg.join("bin")).unwrap();
        fs::write(keg.join("bin/tool"), b"real").unwrap();

        assert!(linker.check_conflicts(&keg).is_ok());
        linker.link_keg(&keg).unwrap();

        let target = fs::read_link(prefix.join("bin/tool")).unwrap();
        assert!(target.to_string_lossy().contains("cellar/tool/1.0.0"));
    }

    #[test]
    fn live_symlink_from_other_formula_still_conflicts() {
        // Replacement is limited to same-formula and dead links; a live link
        // owned by a different formula must keep failing all-or-none.
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let linker = Linker::new(prefix).unwrap();

        let keg1 = setup_keg(&tmp, "alpha");
        linker.link_keg(&keg1).unwrap();

        let keg2 = prefix.join("cellar/beta/1.0.0");
        fs::create_dir_all(keg2.join("bin")).unwrap();
        fs::write(keg2.join("bin/alpha"), b"other").unwrap();

        let result = linker.check_conflicts(&keg2);
        assert!(result.is_err());
        if let Err(Error::LinkConflict { conflicts }) = result {
            assert_eq!(conflicts[0].owned_by.as_deref(), Some("alpha"));
        }
        assert!(linker.link_keg(&keg2).is_err());
        let target = fs::read_link(prefix.join("bin/alpha")).unwrap();
        assert!(target.to_string_lossy().contains("alpha/1.0.0"));
    }

    #[test]
    fn upgrade_expands_legacy_directory_symlink_owned_by_same_formula() {
        // Whole-directory symlinks left by older layouts must be expanded and
        // replaced when the owning formula is upgraded, not reported as a
        // conflict for every file inside.
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let linker = Linker::new(prefix).unwrap();

        let old_keg = prefix.join("cellar/foo/1.0.0");
        fs::create_dir_all(old_keg.join("share/doc/foo")).unwrap();
        fs::write(old_keg.join("share/doc/foo/README"), b"old").unwrap();
        fs::create_dir_all(prefix.join("share/doc")).unwrap();
        std::os::unix::fs::symlink(old_keg.join("share/doc/foo"), prefix.join("share/doc/foo"))
            .unwrap();

        let new_keg = prefix.join("cellar/foo/2.0.0");
        fs::create_dir_all(new_keg.join("share/doc/foo")).unwrap();
        fs::write(new_keg.join("share/doc/foo/README"), b"new").unwrap();

        assert!(linker.check_conflicts(&new_keg).is_ok());
        linker.link_keg(&new_keg).unwrap();

        let readme = prefix.join("share/doc/foo/README");
        let target = fs::read_link(&readme).unwrap();
        assert!(target.to_string_lossy().contains("2.0.0"));
    }

    #[test]
    fn dangling_directory_symlink_is_replaced() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let linker = Linker::new(prefix).unwrap();

        std::os::unix::fs::symlink(
            prefix.join("cellar/foo/0.9.0/share/foo"),
            prefix.join("share/foo"),
        )
        .unwrap();

        let keg = prefix.join("cellar/foo/1.0.0");
        fs::create_dir_all(keg.join("share/foo")).unwrap();
        fs::write(keg.join("share/foo/data.txt"), b"data").unwrap();

        assert!(linker.check_conflicts(&keg).is_ok());
        linker.link_keg(&keg).unwrap();

        assert!(prefix.join("share/foo/data.txt").exists());
    }

    #[test]
    fn keg_name_from_symlink_attributes_dangling_links() {
        let tmp = TempDir::new().unwrap();
        let link = tmp.path().join("gh");
        std::os::unix::fs::symlink(tmp.path().join("cellar/gh/1.0.0/bin/gh"), &link).unwrap();
        assert_eq!(keg_name_from_symlink(&link).as_deref(), Some("gh"));
    }
}
