use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use zb_core::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyStrategy {
    Clonefile,
    Hardlink,
    Copy,
}

pub struct Cellar {
    cellar_dir: PathBuf,
}

impl Cellar {
    pub fn new(root: &Path) -> io::Result<Self> {
        Self::new_at(root.join("cellar"))
    }

    pub fn new_at(cellar_dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&cellar_dir)?;
        Ok(Self { cellar_dir })
    }

    pub fn keg_path(&self, name: &str, version: &str) -> PathBuf {
        self.cellar_dir.join(name).join(version)
    }

    pub fn has_keg(&self, name: &str, version: &str) -> bool {
        self.keg_path(name, version).exists()
    }

    pub fn materialize(
        &self,
        name: &str,
        version: &str,
        store_entry: &Path,
    ) -> Result<PathBuf, Error> {
        let keg_path = self.keg_path(name, version);

        if keg_path.exists() {
            return Ok(keg_path);
        }

        // Create parent directory for the keg
        if let Some(parent) = keg_path.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::StoreCorruption {
                message: format!("failed to create keg parent directory: {e}"),
            })?;
        }

        // Homebrew bottles have structure {name}/{version}/ inside
        // Find the source directory to copy from
        let src_path = find_bottle_content(store_entry, name, version)?;

        // Copy the content to the cellar using best available strategy
        copy_dir_with_fallback(&src_path, &keg_path)?;

        // Patch Homebrew placeholders in Mach-O binaries
        #[cfg(target_os = "macos")]
        patch_homebrew_placeholders(&keg_path, &self.cellar_dir)?;

        // Strip quarantine xattrs and ad-hoc sign Mach-O binaries
        #[cfg(target_os = "macos")]
        codesign_and_strip_xattrs(&keg_path)?;

        Ok(keg_path)
    }

    pub fn remove_keg(&self, name: &str, version: &str) -> Result<(), Error> {
        let keg_path = self.keg_path(name, version);

        if !keg_path.exists() {
            return Ok(());
        }

        fs::remove_dir_all(&keg_path).map_err(|e| Error::StoreCorruption {
            message: format!("failed to remove keg: {e}"),
        })?;

        // Also try to remove the parent (name) directory if it's now empty
        if let Some(parent) = keg_path.parent() {
            let _ = fs::remove_dir(parent); // Ignore error if not empty
        }

        Ok(())
    }
}

/// Find the bottle content directory inside a store entry.
/// Homebrew bottles have structure {name}/{version}/ inside the tarball.
/// This function finds that directory, falling back to the store_entry root
/// if the expected structure isn't found.
fn find_bottle_content(store_entry: &Path, name: &str, version: &str) -> Result<PathBuf, Error> {
    // Try the expected Homebrew structure: {name}/{version}/
    let expected_path = store_entry.join(name).join(version);
    if expected_path.exists() && expected_path.is_dir() {
        return Ok(expected_path);
    }

    // Try just {name}/ (some bottles may have different versioning)
    let name_path = store_entry.join(name);
    if name_path.exists() && name_path.is_dir() {
        // Check if there's a single version directory inside
        if let Ok(entries) = fs::read_dir(&name_path) {
            let dirs: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .collect();
            if dirs.len() == 1 {
                return Ok(dirs[0].path());
            }
        }
        return Ok(name_path);
    }

    // Fall back to store entry root (for flat tarballs or tests)
    Ok(store_entry.to_path_buf())
}

/// Patch @@HOMEBREW_CELLAR@@ and @@HOMEBREW_PREFIX@@ placeholders in Mach-O binaries.
/// Uses rayon for parallel processing.
#[cfg(target_os = "macos")]
fn patch_homebrew_placeholders(keg_path: &Path, cellar_dir: &Path) -> Result<(), Error> {
    use rayon::prelude::*;
    use std::process::Command;

    // Derive prefix from cellar (cellar_dir is typically prefix/Cellar)
    let prefix = cellar_dir
        .parent()
        .unwrap_or(Path::new("/opt/homebrew"));

    let cellar_str = cellar_dir.to_string_lossy().to_string();
    let prefix_str = prefix.to_string_lossy().to_string();

    // Collect all Mach-O files first
    let macho_files: Vec<PathBuf> = walkdir::WalkDir::new(keg_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter(|e| {
            if let Ok(data) = fs::read(e.path()) {
                if data.len() >= 4 {
                    let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                    return matches!(
                        magic,
                        0xfeedface | 0xfeedfacf | 0xcafebabe | 0xcefaedfe | 0xcffaedfe
                    );
                }
            }
            false
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    // Process Mach-O files in parallel
    macho_files.par_iter().for_each(|path| {
        // Get current install names
        let output = Command::new("otool")
            .args(["-L", &path.to_string_lossy()])
            .output();

        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => return,
        };

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Find lines with placeholders and patch them
        for line in stdout.lines() {
            let line = line.trim();
            if line.contains("@@HOMEBREW_CELLAR@@") || line.contains("@@HOMEBREW_PREFIX@@") {
                // Extract the path (before the compatibility version info)
                if let Some(old_path) = line.split_whitespace().next() {
                    let new_path = old_path
                        .replace("@@HOMEBREW_CELLAR@@", &cellar_str)
                        .replace("@@HOMEBREW_PREFIX@@", &prefix_str);

                    // Use install_name_tool to patch
                    let _ = Command::new("install_name_tool")
                        .args(["-change", old_path, &new_path, &path.to_string_lossy()])
                        .output();
                }
            }
        }

        // Also patch the ID if it has placeholders
        let output = Command::new("otool")
            .args(["-D", &path.to_string_lossy()])
            .output();

        if let Ok(o) = output {
            if o.status.success() {
                let stdout = String::from_utf8_lossy(&o.stdout);
                for line in stdout.lines().skip(1) {
                    // Skip first line (filename)
                    let line = line.trim();
                    if line.contains("@@HOMEBREW_CELLAR@@") || line.contains("@@HOMEBREW_PREFIX@@")
                    {
                        let new_id = line
                            .replace("@@HOMEBREW_CELLAR@@", &cellar_str)
                            .replace("@@HOMEBREW_PREFIX@@", &prefix_str);

                        let _ = Command::new("install_name_tool")
                            .args(["-id", &new_id, &path.to_string_lossy()])
                            .output();
                    }
                }
            }
        }
    });

    Ok(())
}

/// Strip quarantine extended attributes and ad-hoc sign unsigned Mach-O binaries.
/// Homebrew bottles from ghcr.io are already adhoc signed, so this is mostly a no-op.
/// We use a fast heuristic: only process binaries that fail signature verification.
#[cfg(target_os = "macos")]
fn codesign_and_strip_xattrs(keg_path: &Path) -> Result<(), Error> {
    use rayon::prelude::*;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    // First, do a quick recursive xattr strip (single command, very fast)
    let _ = Command::new("xattr")
        .args(["-rd", "com.apple.quarantine", &keg_path.to_string_lossy()])
        .stderr(std::process::Stdio::null())
        .output();
    let _ = Command::new("xattr")
        .args(["-rd", "com.apple.provenance", &keg_path.to_string_lossy()])
        .stderr(std::process::Stdio::null())
        .output();

    // Find executables in bin/ directories only (where signing matters)
    // Skip dylibs and other Mach-O files - they inherit signing from their loader
    let bin_files: Vec<PathBuf> = walkdir::WalkDir::new(keg_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            path.is_file() && path.to_string_lossy().contains("/bin/")
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    // Only process files that need signing
    bin_files.par_iter().for_each(|path| {
        // Quick check: is it a Mach-O?
        let data = match fs::read(path) {
            Ok(d) if d.len() >= 4 => d,
            _ => return,
        };
        let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let is_macho = matches!(
            magic,
            0xfeedface | 0xfeedfacf | 0xcafebabe | 0xcefaedfe | 0xcffaedfe
        );
        if !is_macho {
            return;
        }

        // Verify signature - if valid, skip
        let verify = Command::new("codesign")
            .args(["-v", &path.to_string_lossy()])
            .stderr(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .status();

        if verify.map(|s| s.success()).unwrap_or(false) {
            return; // Already signed
        }

        // Get permissions and make writable
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return,
        };
        let original_mode = metadata.permissions().mode();
        let is_readonly = original_mode & 0o200 == 0;

        if is_readonly {
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode | 0o200);
            let _ = fs::set_permissions(path, perms);
        }

        // Sign the binary
        let _ = Command::new("codesign")
            .args(["--force", "--sign", "-", &path.to_string_lossy()])
            .output();

        // Restore permissions
        if is_readonly {
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode);
            let _ = fs::set_permissions(path, perms);
        }
    });

    Ok(())
}

fn copy_dir_with_fallback(src: &Path, dst: &Path) -> Result<(), Error> {
    // Try clonefile first (APFS), then hardlink, then copy
    #[cfg(target_os = "macos")]
    {
        if try_clonefile_dir(src, dst).is_ok() {
            return Ok(());
        }
    }

    // Fall back to recursive copy with hardlink/copy per file
    copy_dir_recursive(src, dst, true)
}

#[cfg(target_os = "macos")]
fn try_clonefile_dir(src: &Path, dst: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let src_cstr = CString::new(src.as_os_str().as_bytes())?;
    let dst_cstr = CString::new(dst.as_os_str().as_bytes())?;

    // clonefile flags: CLONE_NOFOLLOW to not follow symlinks
    const CLONE_NOFOLLOW: u32 = 0x0001;

    unsafe extern "C" {
        fn clonefile(src: *const libc::c_char, dst: *const libc::c_char, flags: u32)
            -> libc::c_int;
    }

    let result = unsafe { clonefile(src_cstr.as_ptr(), dst_cstr.as_ptr(), CLONE_NOFOLLOW) };

    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path, try_hardlink: bool) -> Result<(), Error> {
    fs::create_dir_all(dst).map_err(|e| Error::StoreCorruption {
        message: format!("failed to create directory {}: {e}", dst.display()),
    })?;

    for entry in fs::read_dir(src).map_err(|e| Error::StoreCorruption {
        message: format!("failed to read directory {}: {e}", src.display()),
    })? {
        let entry = entry.map_err(|e| Error::StoreCorruption {
            message: format!("failed to read directory entry: {e}"),
        })?;

        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let file_type = entry.file_type().map_err(|e| Error::StoreCorruption {
            message: format!("failed to get file type: {e}"),
        })?;

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path, try_hardlink)?;
        } else if file_type.is_symlink() {
            let target = fs::read_link(&src_path).map_err(|e| Error::StoreCorruption {
                message: format!("failed to read symlink: {e}"),
            })?;

            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &dst_path).map_err(|e| Error::StoreCorruption {
                message: format!("failed to create symlink: {e}"),
            })?;

            #[cfg(not(unix))]
            fs::copy(&src_path, &dst_path).map_err(|e| Error::StoreCorruption {
                message: format!("failed to copy symlink as file: {e}"),
            })?;
        } else {
            // Try hardlink first, then copy
            if try_hardlink && fs::hard_link(&src_path, &dst_path).is_ok() {
                continue;
            }

            // Fall back to copy
            fs::copy(&src_path, &dst_path).map_err(|e| Error::StoreCorruption {
                message: format!("failed to copy file: {e}"),
            })?;

            // Preserve permissions
            #[cfg(unix)]
            {
                let metadata = fs::metadata(&src_path).map_err(|e| Error::StoreCorruption {
                    message: format!("failed to read metadata: {e}"),
                })?;
                fs::set_permissions(&dst_path, metadata.permissions())
                    .map_err(|e| Error::StoreCorruption {
                        message: format!("failed to set permissions: {e}"),
                    })?;
            }
        }
    }

    Ok(())
}

// For testing - copy without fallback strategies
#[cfg(test)]
fn copy_dir_copy_only(src: &Path, dst: &Path) -> Result<(), Error> {
    copy_dir_recursive(src, dst, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn setup_store_entry(tmp: &TempDir) -> PathBuf {
        let store_entry = tmp.path().join("store/abc123");

        // Create directories first
        fs::create_dir_all(store_entry.join("bin")).unwrap();
        fs::create_dir_all(store_entry.join("lib")).unwrap();

        // Create executable file
        fs::write(store_entry.join("bin/foo"), b"#!/bin/sh\necho foo").unwrap();
        let mut perms = fs::metadata(store_entry.join("bin/foo"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(store_entry.join("bin/foo"), perms).unwrap();

        // Create a regular file
        fs::write(store_entry.join("lib/libfoo.dylib"), b"fake dylib").unwrap();

        // Create a symlink
        std::os::unix::fs::symlink("libfoo.dylib", store_entry.join("lib/libfoo.1.dylib")).unwrap();

        store_entry
    }

    #[test]
    fn tree_reproduced_exactly() {
        let tmp = TempDir::new().unwrap();
        let store_entry = setup_store_entry(&tmp);

        let cellar = Cellar::new(tmp.path()).unwrap();
        let keg_path = cellar.materialize("foo", "1.2.3", &store_entry).unwrap();

        // Check directory structure exists
        assert!(keg_path.exists());
        assert!(keg_path.join("bin").exists());
        assert!(keg_path.join("lib").exists());

        // Check files exist with correct content
        assert_eq!(
            fs::read_to_string(keg_path.join("bin/foo")).unwrap(),
            "#!/bin/sh\necho foo"
        );
        assert_eq!(
            fs::read(keg_path.join("lib/libfoo.dylib")).unwrap(),
            b"fake dylib"
        );

        // Check executable bit preserved
        let perms = fs::metadata(keg_path.join("bin/foo"))
            .unwrap()
            .permissions();
        assert!(perms.mode() & 0o111 != 0, "executable bit not preserved");

        // Check symlink preserved
        let link_path = keg_path.join("lib/libfoo.1.dylib");
        assert!(link_path
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read_link(&link_path).unwrap(),
            PathBuf::from("libfoo.dylib")
        );
    }

    #[test]
    fn second_materialize_is_noop() {
        let tmp = TempDir::new().unwrap();
        let store_entry = setup_store_entry(&tmp);

        let cellar = Cellar::new(tmp.path()).unwrap();

        // First materialize
        let keg_path1 = cellar.materialize("foo", "1.2.3", &store_entry).unwrap();

        // Add a marker file
        fs::write(keg_path1.join("marker.txt"), b"original").unwrap();

        // Second materialize should be no-op
        let keg_path2 = cellar.materialize("foo", "1.2.3", &store_entry).unwrap();
        assert_eq!(keg_path1, keg_path2);

        // Marker should still exist
        assert!(keg_path2.join("marker.txt").exists());
    }

    #[test]
    fn remove_keg_cleans_up() {
        let tmp = TempDir::new().unwrap();
        let store_entry = setup_store_entry(&tmp);

        let cellar = Cellar::new(tmp.path()).unwrap();
        cellar.materialize("foo", "1.2.3", &store_entry).unwrap();

        assert!(cellar.has_keg("foo", "1.2.3"));

        cellar.remove_keg("foo", "1.2.3").unwrap();

        assert!(!cellar.has_keg("foo", "1.2.3"));
    }

    #[test]
    fn keg_path_format() {
        let tmp = TempDir::new().unwrap();
        let cellar = Cellar::new(tmp.path()).unwrap();

        let path = cellar.keg_path("libheif", "2.0.1");
        assert!(path.ends_with("cellar/libheif/2.0.1"));
    }

    #[test]
    fn hardlink_fallback_to_copy_works() {
        // Test that copy fallback works when hardlink fails
        // (e.g., across different filesystems)
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();

        let src = tmp1.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("test.txt"), b"test content").unwrap();

        let dst = tmp2.path().join("dst");

        // Use copy_dir_copy_only to skip hardlink attempts
        copy_dir_copy_only(&src, &dst).unwrap();

        assert_eq!(
            fs::read_to_string(dst.join("test.txt")).unwrap(),
            "test content"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn clonefile_fallback_works() {
        // On APFS, clonefile should work
        let tmp = TempDir::new().unwrap();
        let store_entry = setup_store_entry(&tmp);

        let cellar = Cellar::new(tmp.path()).unwrap();
        let keg_path = cellar.materialize("clone", "1.0.0", &store_entry).unwrap();

        // Verify content is correct regardless of which strategy was used
        assert_eq!(
            fs::read_to_string(keg_path.join("bin/foo")).unwrap(),
            "#!/bin/sh\necho foo"
        );
    }
}
