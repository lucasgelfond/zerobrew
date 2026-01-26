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
        patch_homebrew_placeholders(&keg_path, &self.cellar_dir, name, version)?;

        // Patch Homebrew placeholders in text files (shell scripts, etc.)
        #[cfg(target_os = "macos")]
        patch_text_file_placeholders(&keg_path, &self.cellar_dir, name, version)?;

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
/// Also fixes version mismatches where a bottle references a different version of itself.
/// Uses rayon for parallel processing.
#[cfg(target_os = "macos")]
fn patch_homebrew_placeholders(
    keg_path: &Path,
    cellar_dir: &Path,
    pkg_name: &str,
    pkg_version: &str,
) -> Result<(), Error> {
    use rayon::prelude::*;
    use regex::Regex;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Derive prefix from cellar (cellar_dir is typically prefix/Cellar)
    let prefix = cellar_dir.parent().unwrap_or(Path::new("/opt/homebrew"));

    let cellar_str = cellar_dir.to_string_lossy().to_string();
    let prefix_str = prefix.to_string_lossy().to_string();

    // Regex to match version mismatches in paths like /Cellar/ffmpeg/8.0.1_1/
    // We'll fix references to this package with wrong versions
    let version_pattern = format!(r"(/{}/)([^/]+)(/)", regex::escape(pkg_name));
    let version_regex = Regex::new(&version_pattern).ok();

    // Collect all Mach-O files first (skip symlinks to avoid double-processing)
    let macho_files: Vec<PathBuf> = walkdir::WalkDir::new(keg_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            // Skip symlinks - only process actual files
            e.file_type().is_file()
        })
        .filter(|e| {
            if let Ok(data) = fs::read(e.path())
                && data.len() >= 4
            {
                let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                return matches!(
                    magic,
                    0xfeedface | 0xfeedfacf | 0xcafebabe | 0xcefaedfe | 0xcffaedfe
                );
            }
            false
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    // Track patch failures
    let patch_failures = AtomicUsize::new(0);

    // Helper to patch a single path reference
    let patch_path = |old_path: &str| -> Option<String> {
        let mut new_path = old_path.to_string();
        let mut changed = false;

        // Replace Homebrew placeholders
        if old_path.contains("@@HOMEBREW_CELLAR@@") || old_path.contains("@@HOMEBREW_PREFIX@@") {
            new_path = new_path
                .replace("@@HOMEBREW_CELLAR@@", &cellar_str)
                .replace("@@HOMEBREW_PREFIX@@", &prefix_str);
            changed = true;
        }

        // Fix version mismatches for this package
        if let Some(re) = &version_regex
            && re.is_match(&new_path)
        {
            let replacement = format!("/{}/{}/", pkg_name, pkg_version);
            let fixed = re.replace(&new_path, |caps: &regex::Captures| {
                let matched_version = &caps[2];
                if matched_version != pkg_version {
                    replacement.clone()
                } else {
                    caps[0].to_string()
                }
            });
            if fixed != new_path {
                new_path = fixed.to_string();
                changed = true;
            }
        }

        if changed && new_path != old_path {
            Some(new_path)
        } else {
            None
        }
    };

    // Process Mach-O files in parallel
    macho_files.par_iter().for_each(|path| {
        // Get file permissions and make writable if needed
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return,
        };
        let original_mode = metadata.permissions().mode();
        let is_readonly = original_mode & 0o200 == 0;

        // Make writable for patching
        if is_readonly {
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode | 0o200);
            if fs::set_permissions(path, perms).is_err() {
                patch_failures.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }

        let mut patched_any = false;

        // Get and patch library dependencies (-L)
        if let Ok(output) = Command::new("otool")
            .args(["-L", &path.to_string_lossy()])
            .output()
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if let Some(old_path) = line.split_whitespace().next()
                    && let Some(new_path) = patch_path(old_path)
                {
                    let result = Command::new("install_name_tool")
                        .args(["-change", old_path, &new_path, &path.to_string_lossy()])
                        .output();
                    if result.is_ok() {
                        patched_any = true;
                    } else {
                        patch_failures.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }

        // Get and patch install name ID (-D)
        if let Ok(output) = Command::new("otool")
            .args(["-D", &path.to_string_lossy()])
            .output()
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().skip(1) {
                // Skip first line (filename)
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Some(new_id) = patch_path(line) {
                    let result = Command::new("install_name_tool")
                        .args(["-id", &new_id, &path.to_string_lossy()])
                        .output();
                    if result.is_ok() {
                        patched_any = true;
                    } else {
                        patch_failures.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }

        // Re-sign if we patched anything (patching invalidates code signature)
        if patched_any {
            let _ = Command::new("codesign")
                .args(["--force", "--sign", "-", &path.to_string_lossy()])
                .output();
        }

        // Restore original permissions
        if is_readonly {
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode);
            let _ = fs::set_permissions(path, perms);
        }
    });

    let failures = patch_failures.load(Ordering::Relaxed);
    if failures > 0 {
        return Err(Error::StoreCorruption {
            message: format!(
                "failed to patch {} Mach-O files in {}",
                failures,
                keg_path.display()
            ),
        });
    }

    Ok(())
}

/// Patch @@HOMEBREW_CELLAR@@ and @@HOMEBREW_PREFIX@@ placeholders in text files.
/// This handles shell scripts and other text files that contain Homebrew placeholders.
/// Uses rayon for parallel processing.
#[cfg(target_os = "macos")]
fn patch_text_file_placeholders(
    keg_path: &Path,
    cellar_dir: &Path,
    _pkg_name: &str,
    _pkg_version: &str,
) -> Result<(), Error> {
    use rayon::prelude::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Derive prefix from cellar (cellar_dir is typically prefix/cellar)
    let prefix = cellar_dir.parent().unwrap_or(Path::new("/opt/homebrew"));

    let cellar_str = cellar_dir.to_string_lossy().to_string();
    let prefix_str = prefix.to_string_lossy().to_string();

    // Collect text files that contain placeholders (skip symlinks and binaries)
    let text_files: Vec<PathBuf> = walkdir::WalkDir::new(keg_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            if let Ok(data) = fs::read(e.path()) {
                // Skip empty files
                if data.is_empty() {
                    return false;
                }
                // Skip binary files (check magic bytes for Mach-O)
                if data.len() >= 4 {
                    let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                    if matches!(
                        magic,
                        0xfeedface | 0xfeedfacf | 0xcafebabe | 0xcefaedfe | 0xcffaedfe
                    ) {
                        return false;
                    }
                }
                // Check if file contains placeholders we need to fix
                let content = String::from_utf8_lossy(&data);
                content.contains("@@HOMEBREW_CELLAR@@") || content.contains("@@HOMEBREW_PREFIX@@")
            } else {
                false
            }
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    // Track patch failures
    let patch_failures = AtomicUsize::new(0);

    // Process text files in parallel
    text_files.par_iter().for_each(|path| {
        // Read file content
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => {
                // File might not be valid UTF-8, skip it
                return;
            }
        };

        let mut new_content = content.clone();

        // Replace Homebrew placeholders
        new_content = new_content
            .replace("@@HOMEBREW_CELLAR@@", &cellar_str)
            .replace("@@HOMEBREW_PREFIX@@", &prefix_str);

        // Only write if content changed
        if new_content == content {
            return;
        }

        // Get file permissions and make writable if needed
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => {
                patch_failures.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let original_mode = metadata.permissions().mode();
        let is_readonly = original_mode & 0o200 == 0;

        // Make writable for patching
        if is_readonly {
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode | 0o200);
            if fs::set_permissions(path, perms).is_err() {
                patch_failures.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }

        // Write patched content
        if fs::write(path, &new_content).is_err() {
            patch_failures.fetch_add(1, Ordering::Relaxed);
        }

        // Restore original permissions
        if is_readonly {
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode);
            let _ = fs::set_permissions(path, perms);
        }
    });

    let failures = patch_failures.load(Ordering::Relaxed);
    if failures > 0 {
        return Err(Error::StoreCorruption {
            message: format!(
                "failed to patch {} text files in {}",
                failures,
                keg_path.display()
            ),
        });
    }

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
                fs::set_permissions(&dst_path, metadata.permissions()).map_err(|e| {
                    Error::StoreCorruption {
                        message: format!("failed to set permissions: {e}"),
                    }
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
        assert!(
            link_path
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
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

    #[test]
    fn version_mismatch_regex_fixes_paths() {
        use regex::Regex;

        let pkg_name = "ffmpeg";
        let pkg_version = "8.0.1_2";

        // Create the version mismatch regex
        let version_pattern = format!(r"(/{}/)([^/]+)(/)", regex::escape(pkg_name));
        let version_regex = Regex::new(&version_pattern).unwrap();

        // Test case: path with wrong version
        let old_path = "/opt/zerobrew/prefix/Cellar/ffmpeg/8.0.1_1/lib/libavdevice.62.dylib";
        let replacement = format!("/{}/{}/", pkg_name, pkg_version);

        let fixed = version_regex.replace(old_path, |caps: &regex::Captures| {
            let matched_version = &caps[2];
            if matched_version != pkg_version {
                replacement.clone()
            } else {
                caps[0].to_string()
            }
        });

        assert_eq!(
            fixed,
            "/opt/zerobrew/prefix/Cellar/ffmpeg/8.0.1_2/lib/libavdevice.62.dylib"
        );

        // Test case: path with correct version (should not change)
        let correct_path = "/opt/zerobrew/prefix/Cellar/ffmpeg/8.0.1_2/lib/libavdevice.62.dylib";
        let fixed2 = version_regex.replace(correct_path, |caps: &regex::Captures| {
            let matched_version = &caps[2];
            if matched_version != pkg_version {
                replacement.clone()
            } else {
                caps[0].to_string()
            }
        });

        assert_eq!(fixed2, correct_path);

        // Test case: path for different package (should not change)
        let other_path = "/opt/zerobrew/prefix/Cellar/libvpx/1.0.0/lib/libvpx.dylib";
        let fixed3 = version_regex.replace(other_path, |caps: &regex::Captures| {
            let matched_version = &caps[2];
            if matched_version != pkg_version {
                replacement.clone()
            } else {
                caps[0].to_string()
            }
        });

        assert_eq!(fixed3, other_path);
    }
}
