use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use zb_core::{ConflictedLink, Error};

const LINK_DIRS: &[&str] = &["bin", "lib", "libexec", "include", "share", "etc"];

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

fn keg_name_from_symlink(dst: &Path) -> Option<String> {
    let target = fs::read_link(dst).ok()?;
    let resolved = if target.is_relative() {
        dst.parent().unwrap_or(Path::new("")).join(&target)
    } else {
        target
    };
    let canonical = fs::canonicalize(&resolved).ok()?;
    keg_name_from_path(&canonical)
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
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            if file_type.is_dir() {
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
            let src_path = entry.path();
            let matching_old = old_target.join(entry.file_name());
            let dst_path = dst.join(entry.file_name());

            if entry.file_type().is_ok_and(|ft| ft.is_dir()) {
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
            fs::create_dir_all(dst).map_err(|e| Error::StoreCorruption {
                message: e.to_string(),
            })?;
        }

        for entry in fs::read_dir(src).map_err(|e| Error::StoreCorruption {
            message: e.to_string(),
        })? {
            let entry = entry.map_err(|e| Error::StoreCorruption {
                message: e.to_string(),
            })?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            let file_type = entry.file_type().map_err(|e| Error::StoreCorruption {
                message: e.to_string(),
            })?;

            if file_type.is_dir() {
                if dst_path.symlink_metadata().is_ok() && dst_path.is_symlink() {
                    let old_target =
                        fs::read_link(&dst_path).map_err(|e| Error::StoreCorruption {
                            message: e.to_string(),
                        })?;
                    let _ = fs::remove_file(&dst_path);
                    Self::link_recursive(&old_target, &dst_path)?;
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
            std::os::unix::fs::symlink(&src_path, &dst_path).map_err(|e| {
                Error::StoreCorruption {
                    message: e.to_string(),
                }
            })?;
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
        for dir_name in LINK_DIRS {
            let src_dir = keg_path.join(dir_name);
            let dst_dir = self.prefix.join(dir_name);
            if src_dir.exists() {
                unlinked.extend(Self::unlink_recursive(&src_dir, &dst_dir)?);
            }
        }
        Ok(unlinked)
    }

    fn unlink_recursive(src: &Path, dst: &Path) -> Result<Vec<PathBuf>, Error> {
        let mut unlinked = Vec::new();
        if !src.exists() || !dst.exists() {
            return Ok(unlinked);
        }
        for entry in fs::read_dir(src).map_err(|e| Error::StoreCorruption {
            message: e.to_string(),
        })? {
            let entry = entry.map_err(|e| Error::StoreCorruption {
                message: e.to_string(),
            })?;
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
        std::os::unix::fs::symlink(keg_path, &opt_link).map_err(|e| Error::StoreCorruption {
            message: e.to_string(),
        })?;
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
    fn links_libexec_directory() {
        let tmp = TempDir::new().unwrap();
        let keg = tmp.path().join("cellar/git/2.52.0");
        let libexec_dir = keg.join("libexec/git-core");
        fs::create_dir_all(&libexec_dir).unwrap();

        let helper = libexec_dir.join("git-remote-https");
        fs::write(&helper, b"#!/bin/sh\necho helper").unwrap();
        fs::set_permissions(&helper, PermissionsExt::from_mode(0o755)).unwrap();

        let linker = Linker::new(tmp.path()).unwrap();
        linker.link_keg(&keg).unwrap();

        let linked_helper = tmp.path().join("libexec/git-core/git-remote-https");
        assert!(linked_helper.exists(), "git-remote-https should be linked");
        assert!(linked_helper.is_symlink(), "should be a symlink");
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
}
