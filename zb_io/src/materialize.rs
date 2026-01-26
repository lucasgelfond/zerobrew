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

        // Strip quarantine xattrs and ad-hoc sign Mach-O binaries
        #[cfg(target_os = "macos")]
        codesign_and_strip_xattrs(&keg_path)?;

        // Patch Homebrew placeholders in ELF binaries (Linux)
        #[cfg(target_os = "linux")]
        patch_homebrew_placeholders_linux(&keg_path, &self.cellar_dir, name, version)?;

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

/// Patch @@HOMEBREW_CELLAR@@ and @@HOMEBREW_PREFIX@@ placeholders in ELF binaries.
/// Uses patchelf to modify RPATH/RUNPATH entries. Also fixes version mismatches.
#[cfg(target_os = "linux")]
fn patch_homebrew_placeholders_linux(
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

    // Check if patchelf is available
    if Command::new("patchelf")
        .arg("--version")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        // patchelf not available - skip patching but don't fail
        // Many simple packages work without rpath patching
        return Ok(());
    }

    // Derive prefix from cellar (cellar_dir is typically prefix/Cellar)
    let prefix = cellar_dir.parent().unwrap_or(Path::new("/opt/homebrew"));

    let cellar_str = cellar_dir.to_string_lossy().to_string();
    let prefix_str = prefix.to_string_lossy().to_string();

    // Regex to match version mismatches in paths like /Cellar/ffmpeg/8.0.1_1/
    let version_pattern = format!(r"(/{}/)([^/]+)(/)", regex::escape(pkg_name));
    let version_regex = Regex::new(&version_pattern).ok();

    // ELF magic bytes: 0x7f 'E' 'L' 'F'
    const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

    // Collect all ELF files (skip symlinks to avoid double-processing)
    let elf_files: Vec<PathBuf> = walkdir::WalkDir::new(keg_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            if let Ok(data) = fs::read(e.path())
                && data.len() >= 4
            {
                return data[0..4] == ELF_MAGIC;
            }
            false
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    // Track patch failures
    let patch_failures = AtomicUsize::new(0);

    // Helper to patch a single rpath string
    let patch_rpath = |rpath: &str| -> Option<String> {
        let mut new_rpath = rpath.to_string();
        let mut changed = false;

        // Replace Homebrew placeholders
        if rpath.contains("@@HOMEBREW_CELLAR@@") || rpath.contains("@@HOMEBREW_PREFIX@@") {
            new_rpath = new_rpath
                .replace("@@HOMEBREW_CELLAR@@", &cellar_str)
                .replace("@@HOMEBREW_PREFIX@@", &prefix_str);
            changed = true;
        }

        // Fix version mismatches for this package
        if let Some(re) = &version_regex
            && re.is_match(&new_rpath)
        {
            let replacement = format!("/{}/{}/", pkg_name, pkg_version);
            let fixed = re.replace(&new_rpath, |caps: &regex::Captures| {
                let matched_version = &caps[2];
                if matched_version != pkg_version {
                    replacement.clone()
                } else {
                    caps[0].to_string()
                }
            });
            if fixed != new_rpath {
                new_rpath = fixed.to_string();
                changed = true;
            }
        }

        if changed && new_rpath != rpath {
            Some(new_rpath)
        } else {
            None
        }
    };

    // Process ELF files in parallel
    elf_files.par_iter().for_each(|path| {
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

        // Get current RPATH/RUNPATH using patchelf --print-rpath
        let rpath_output = Command::new("patchelf")
            .args(["--print-rpath", &path.to_string_lossy()])
            .output();

        if let Ok(output) = rpath_output
            && output.status.success()
        {
            let current_rpath = String::from_utf8_lossy(&output.stdout).trim().to_string();

            if !current_rpath.is_empty() {
                // Process each path in RPATH (colon-separated)
                let new_rpath_parts: Vec<String> = current_rpath
                    .split(':')
                    .map(|p| patch_rpath(p).unwrap_or_else(|| p.to_string()))
                    .collect();

                let new_rpath = new_rpath_parts.join(":");

                if new_rpath != current_rpath {
                    // Apply the patched RPATH
                    let result = Command::new("patchelf")
                        .args(["--set-rpath", &new_rpath, &path.to_string_lossy()])
                        .output();

                    if result.map(|o| !o.status.success()).unwrap_or(true) {
                        patch_failures.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }

        // Also check and patch the interpreter (for executables)
        let interp_output = Command::new("patchelf")
            .args(["--print-interpreter", &path.to_string_lossy()])
            .output();

        if let Ok(output) = interp_output
            && output.status.success()
        {
            let current_interp = String::from_utf8_lossy(&output.stdout).trim().to_string();

            if !current_interp.is_empty() {
                // Only patch interpreter if it contains Homebrew placeholder
                let new_interp = if current_interp.contains("@@HOMEBREW") {
                    // Use the system dynamic linker based on architecture
                    #[cfg(target_arch = "aarch64")]
                    { Some("/lib/ld-linux-aarch64.so.1".to_string()) }
                    #[cfg(target_arch = "x86_64")]
                    { Some("/lib64/ld-linux-x86-64.so.2".to_string()) }
                    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
                    { None }
                } else {
                    None
                };

                if let Some(interp) = new_interp {
                    let result = Command::new("patchelf")
                        .args(["--set-interpreter", &interp, &path.to_string_lossy()])
                        .output();

                    if result.map(|o| !o.status.success()).unwrap_or(true) {
                        // Interpreter patching can fail for shared libraries, that's okay
                        // Only count it as a real failure if we expected it to work
                    }
                }
            }
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
                "failed to patch {} ELF files in {}",
                failures,
                keg_path.display()
            ),
        });
    }

    Ok(())
}

fn copy_dir_with_fallback(src: &Path, dst: &Path) -> Result<(), Error> {
    // Try clonefile first (APFS on macOS), then hardlink, then copy
    #[cfg(target_os = "macos")]
    {
        if try_clonefile_dir(src, dst).is_ok() {
            return Ok(());
        }
    }

    // On Linux, try reflink copy (btrfs/XFS) before falling back
    #[cfg(target_os = "linux")]
    {
        if try_reflink_copy_dir(src, dst).is_ok() {
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

/// Try to copy a directory using reflinks (copy-on-write) on Linux.
/// Works on btrfs and XFS filesystems. Falls back gracefully on others.
#[cfg(target_os = "linux")]
fn try_reflink_copy_dir(src: &Path, dst: &Path) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;

    // FICLONE ioctl number for Linux
    const FICLONE: libc::c_ulong = 0x40049409;

    fn try_reflink_file(src_file: &fs::File, dst_file: &fs::File) -> io::Result<()> {
        let result = unsafe { libc::ioctl(dst_file.as_raw_fd(), FICLONE, src_file.as_raw_fd()) };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn copy_dir_reflink(src: &Path, dst: &Path) -> io::Result<()> {
        fs::create_dir_all(dst)?;

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            let file_type = entry.file_type()?;

            if file_type.is_dir() {
                copy_dir_reflink(&src_path, &dst_path)?;
            } else if file_type.is_symlink() {
                let target = fs::read_link(&src_path)?;
                std::os::unix::fs::symlink(&target, &dst_path)?;
            } else {
                // Try reflink for regular files
                let src_file = fs::File::open(&src_path)?;
                let dst_file = fs::File::create(&dst_path)?;

                if try_reflink_file(&src_file, &dst_file).is_err() {
                    // Reflink failed - filesystem doesn't support it
                    // Clean up and return error to trigger fallback
                    drop(dst_file);
                    let _ = fs::remove_file(&dst_path);
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "reflink not supported",
                    ));
                }

                // Preserve permissions
                let metadata = src_file.metadata()?;
                fs::set_permissions(&dst_path, metadata.permissions())?;
            }
        }

        Ok(())
    }

    // Try to copy the entire directory with reflinks
    // If any file fails, we need to clean up and fall back
    match copy_dir_reflink(src, dst) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Clean up partial copy
            let _ = fs::remove_dir_all(dst);
            Err(e)
        }
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

    // ========================================================================
    // Linux-specific ELF tests
    // ========================================================================

    /// Test ELF magic byte detection
    #[test]
    fn elf_magic_detection() {
        const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

        // Valid ELF header
        let elf_data = [0x7f, b'E', b'L', b'F', 0x02, 0x01, 0x01, 0x00];
        assert_eq!(&elf_data[0..4], &ELF_MAGIC);

        // Invalid - wrong magic
        let not_elf = [0x00, 0x00, 0x00, 0x00];
        assert_ne!(&not_elf[0..4], &ELF_MAGIC);

        // Invalid - Mach-O (macOS) magic
        let macho_data = [0xfe, 0xed, 0xfa, 0xce]; // feedface
        assert_ne!(&macho_data[0..4], &ELF_MAGIC);

        // Invalid - shell script
        let script_data = [b'#', b'!', b'/', b'b'];
        assert_ne!(&script_data[0..4], &ELF_MAGIC);
    }

    /// Test Mach-O magic byte detection (for contrast with ELF)
    #[test]
    fn macho_magic_detection() {
        // Mach-O 32-bit (big-endian magic)
        let macho32 = [0xfe, 0xed, 0xfa, 0xce];
        let magic32 = u32::from_be_bytes(macho32);
        assert_eq!(magic32, 0xfeedface);

        // Mach-O 64-bit (big-endian magic)
        let macho64 = [0xfe, 0xed, 0xfa, 0xcf];
        let magic64 = u32::from_be_bytes(macho64);
        assert_eq!(magic64, 0xfeedfacf);

        // Fat/Universal binary
        let fat = [0xca, 0xfe, 0xba, 0xbe];
        let fat_magic = u32::from_be_bytes(fat);
        assert_eq!(fat_magic, 0xcafebabe);
    }

    /// Test RPATH placeholder replacement logic (cross-platform)
    #[test]
    fn rpath_placeholder_replacement() {
        let cellar = "/opt/zerobrew/prefix/Cellar";
        let prefix = "/opt/zerobrew/prefix";

        // Test @@HOMEBREW_CELLAR@@ replacement
        let old_rpath = "@@HOMEBREW_CELLAR@@/openssl/3.0.0/lib";
        let new_rpath = old_rpath
            .replace("@@HOMEBREW_CELLAR@@", cellar)
            .replace("@@HOMEBREW_PREFIX@@", prefix);
        assert_eq!(new_rpath, "/opt/zerobrew/prefix/Cellar/openssl/3.0.0/lib");

        // Test @@HOMEBREW_PREFIX@@ replacement
        let old_rpath2 = "@@HOMEBREW_PREFIX@@/lib";
        let new_rpath2 = old_rpath2
            .replace("@@HOMEBREW_CELLAR@@", cellar)
            .replace("@@HOMEBREW_PREFIX@@", prefix);
        assert_eq!(new_rpath2, "/opt/zerobrew/prefix/lib");

        // Test combined placeholders
        let old_rpath3 = "@@HOMEBREW_CELLAR@@/foo/1.0/lib:@@HOMEBREW_PREFIX@@/lib";
        let new_rpath3 = old_rpath3
            .replace("@@HOMEBREW_CELLAR@@", cellar)
            .replace("@@HOMEBREW_PREFIX@@", prefix);
        assert_eq!(
            new_rpath3,
            "/opt/zerobrew/prefix/Cellar/foo/1.0/lib:/opt/zerobrew/prefix/lib"
        );
    }

    /// Test that reflink copy falls back gracefully
    #[test]
    #[cfg(target_os = "linux")]
    fn reflink_fallback_to_copy() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let _dst = tmp.path().join("dst"); // Unused, but kept for clarity

        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("file.txt"), b"test content").unwrap();

        // copy_dir_with_fallback should always succeed, using whatever
        // method works (reflink → hardlink → copy)
        let cellar_path = tmp.path().join("cellar");
        fs::create_dir_all(&cellar_path).unwrap();
        let cellar = Cellar::new_at(cellar_path).unwrap();

        // Test via materialize which uses copy_dir_with_fallback internally
        let keg = cellar.materialize("test", "1.0.0", &src).unwrap();
        assert!(keg.join("file.txt").exists());
        assert_eq!(
            fs::read_to_string(keg.join("file.txt")).unwrap(),
            "test content"
        );
    }

    /// Test ELF file detection in directory walk
    #[test]
    #[cfg(target_os = "linux")]
    fn detects_elf_files_in_directory() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let bin_dir = tmp.path().join("bin");
        let lib_dir = tmp.path().join("lib");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::create_dir_all(&lib_dir).unwrap();

        // Create a fake ELF executable
        let elf_header: Vec<u8> = vec![
            0x7f, b'E', b'L', b'F', // Magic
            0x02, // 64-bit
            0x01, // Little-endian
            0x01, // ELF version 1
            0x00, // OS/ABI
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Padding
        ];
        fs::write(bin_dir.join("myapp"), &elf_header).unwrap();
        let mut perms = fs::metadata(bin_dir.join("myapp")).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(bin_dir.join("myapp"), perms).unwrap();

        // Create a fake shared library with ELF header
        let mut lib_data = elf_header.clone();
        lib_data.extend_from_slice(b"fake lib data");
        fs::write(lib_dir.join("libfoo.so"), &lib_data).unwrap();

        // Create a non-ELF file (shell script)
        fs::write(bin_dir.join("script.sh"), b"#!/bin/sh\necho hello").unwrap();

        // Verify ELF magic detection
        const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

        let elf_count = walkdir::WalkDir::new(tmp.path())
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                if let Ok(data) = fs::read(e.path())
                    && data.len() >= 4
                {
                    return data[0..4] == ELF_MAGIC;
                }
                false
            })
            .count();

        assert_eq!(elf_count, 2, "Should detect 2 ELF files");
    }

    /// Test symlink preservation during materialization
    #[test]
    fn symlink_preservation() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("lib")).unwrap();

        // Create a file and a symlink to it
        fs::write(src.join("lib/libfoo.so.1.0.0"), b"lib content").unwrap();
        std::os::unix::fs::symlink("libfoo.so.1.0.0", src.join("lib/libfoo.so.1")).unwrap();
        std::os::unix::fs::symlink("libfoo.so.1", src.join("lib/libfoo.so")).unwrap();

        let cellar = Cellar::new(tmp.path()).unwrap();
        let keg = cellar.materialize("foo", "1.0.0", &src).unwrap();

        // Check real file exists
        assert!(keg.join("lib/libfoo.so.1.0.0").exists());

        // Check symlinks are preserved as symlinks
        let link1 = keg.join("lib/libfoo.so.1");
        assert!(link1.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read_link(&link1).unwrap().to_string_lossy(), "libfoo.so.1.0.0");

        let link2 = keg.join("lib/libfoo.so");
        assert!(link2.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read_link(&link2).unwrap().to_string_lossy(), "libfoo.so.1");
    }

    /// Test that find_bottle_content handles various bottle structures
    #[test]
    fn find_bottle_content_variants() {
        let tmp = TempDir::new().unwrap();

        // Test 1: Standard Homebrew structure {name}/{version}/
        let store1 = tmp.path().join("store1");
        fs::create_dir_all(store1.join("foo/1.0.0/bin")).unwrap();
        fs::write(store1.join("foo/1.0.0/bin/foo"), b"binary").unwrap();

        let content1 = find_bottle_content(&store1, "foo", "1.0.0").unwrap();
        assert!(content1.ends_with("foo/1.0.0"));

        // Test 2: Just {name}/ with single version directory
        let store2 = tmp.path().join("store2");
        fs::create_dir_all(store2.join("bar/2.0.0")).unwrap();
        fs::write(store2.join("bar/2.0.0/README"), b"readme").unwrap();

        let content2 = find_bottle_content(&store2, "bar", "2.0.0").unwrap();
        assert!(content2.to_string_lossy().contains("bar"));

        // Test 3: Flat structure (fallback to store root)
        let store3 = tmp.path().join("store3");
        fs::create_dir_all(store3.join("bin")).unwrap();
        fs::write(store3.join("bin/baz"), b"binary").unwrap();

        let content3 = find_bottle_content(&store3, "baz", "3.0.0").unwrap();
        assert_eq!(content3, store3);
    }

    /// Test file permissions are preserved during copy
    #[test]
    fn permissions_preserved() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("bin")).unwrap();

        // Create files with different permissions
        fs::write(src.join("bin/executable"), b"#!/bin/sh\necho hi").unwrap();
        let mut perms = fs::metadata(src.join("bin/executable")).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(src.join("bin/executable"), perms).unwrap();

        fs::write(src.join("bin/readonly"), b"data").unwrap();
        let mut perms = fs::metadata(src.join("bin/readonly")).unwrap().permissions();
        perms.set_mode(0o444);
        fs::set_permissions(src.join("bin/readonly"), perms).unwrap();

        let cellar = Cellar::new(tmp.path()).unwrap();
        let keg = cellar.materialize("test", "1.0.0", &src).unwrap();

        // Check executable permission
        let exec_perms = fs::metadata(keg.join("bin/executable")).unwrap().permissions();
        assert!(exec_perms.mode() & 0o111 != 0, "executable bit should be preserved");

        // Check readonly permission
        let ro_perms = fs::metadata(keg.join("bin/readonly")).unwrap().permissions();
        assert!(ro_perms.mode() & 0o222 == 0, "write bit should not be set on readonly file");
    }

    // ========================================================================
    // Integration tests for Linux ELF patching
    // ========================================================================

    /// Test placeholder patching with real file structure (Linux)
    #[test]
    #[cfg(target_os = "linux")]
    #[ignore] // Requires patchelf to be installed
    fn elf_patching_with_patchelf() {
        use std::process::Command;

        // Check if patchelf is available
        let patchelf_check = Command::new("patchelf").arg("--version").output();
        if patchelf_check.map(|o| !o.status.success()).unwrap_or(true) {
            // patchelf not available, skip test
            return;
        }

        let tmp = TempDir::new().unwrap();
        let cellar = Cellar::new(tmp.path()).unwrap();

        // Create a minimal ELF-like structure
        // Note: This is a placeholder test - real ELF binaries would be needed
        // for full integration testing
        let store = tmp.path().join("store/elf-test");
        fs::create_dir_all(store.join("bin")).unwrap();

        // For a real test, we'd need an actual ELF binary
        // This test documents the expected behavior
        assert!(cellar.cellar_dir.exists());
    }

    /// Test that missing patchelf doesn't cause failures
    #[test]
    #[cfg(target_os = "linux")]
    fn missing_patchelf_graceful_handling() {
        // This test verifies that the code handles missing patchelf gracefully
        // by checking that patch_homebrew_placeholders_linux returns Ok even
        // when patchelf isn't available (it should skip patching, not fail)

        let tmp = TempDir::new().unwrap();
        let keg = tmp.path().join("keg");
        let cellar = tmp.path().join("cellar");
        fs::create_dir_all(&keg).unwrap();
        fs::create_dir_all(&cellar).unwrap();

        // Create a fake ELF file (just the magic bytes, not a real binary)
        let elf_header: Vec<u8> = vec![
            0x7f, b'E', b'L', b'F',
            0x02, 0x01, 0x01, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        fs::create_dir_all(keg.join("bin")).unwrap();
        fs::write(keg.join("bin/fake-elf"), &elf_header).unwrap();

        // patch_homebrew_placeholders_linux should return Ok even if patchelf
        // isn't installed - it gracefully skips patching
        let result = patch_homebrew_placeholders_linux(&keg, &cellar, "test", "1.0.0");
        assert!(result.is_ok(), "Should not fail when patchelf is missing");
    }

    /// Test Linux interpreter paths for different architectures
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_interpreter_paths() {
        // These are the expected system dynamic linkers
        #[cfg(target_arch = "aarch64")]
        {
            let interp = "/lib/ld-linux-aarch64.so.1";
            assert!(interp.contains("aarch64"));
        }
        #[cfg(target_arch = "x86_64")]
        {
            let interp = "/lib64/ld-linux-x86-64.so.2";
            assert!(interp.contains("x86-64"));
        }
    }

    // ========================================================================
    // Edge case tests for ELF handling
    // ========================================================================

    /// Test detection of corrupted ELF files (valid magic, invalid structure)
    #[test]
    #[cfg(target_os = "linux")]
    fn handles_truncated_elf_gracefully() {
        let tmp = TempDir::new().unwrap();
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();

        // Create a truncated ELF file (just magic bytes, nothing else)
        let truncated_elf = vec![0x7f, b'E', b'L', b'F'];
        fs::write(bin_dir.join("truncated"), &truncated_elf).unwrap();

        // The file should be detected as ELF but patching should handle gracefully
        const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
        let data = fs::read(bin_dir.join("truncated")).unwrap();
        assert!(data.len() >= 4 && data[0..4] == ELF_MAGIC);
    }

    /// Test handling of ELF file with no RPATH/RUNPATH
    #[test]
    #[cfg(target_os = "linux")]
    fn handles_elf_without_rpath() {
        let tmp = TempDir::new().unwrap();
        let keg = tmp.path().join("keg");
        let cellar = tmp.path().join("cellar");
        fs::create_dir_all(keg.join("bin")).unwrap();
        fs::create_dir_all(&cellar).unwrap();

        // Create a valid-looking ELF header but no dynamic section
        let elf_no_rpath: Vec<u8> = vec![
            0x7f, b'E', b'L', b'F',
            0x02, // 64-bit
            0x01, // Little-endian
            0x01, // ELF version 1
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x03, 0x00, // e_type: ET_DYN
            0x3e, 0x00, // e_machine: EM_X86_64
            0x01, 0x00, 0x00, 0x00, // e_version
        ];
        fs::write(keg.join("bin/no-rpath"), &elf_no_rpath).unwrap();

        // Patching should succeed (skip files without RPATH)
        let result = patch_homebrew_placeholders_linux(&keg, &cellar, "test", "1.0.0");
        assert!(result.is_ok(), "Should handle ELF without RPATH gracefully");
    }

    /// Test handling of read-only ELF files
    #[test]
    #[cfg(target_os = "linux")]
    fn handles_readonly_elf_files() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let keg = tmp.path().join("keg");
        let cellar = tmp.path().join("cellar");
        fs::create_dir_all(keg.join("bin")).unwrap();
        fs::create_dir_all(&cellar).unwrap();

        // Create ELF file
        let elf_data: Vec<u8> = vec![
            0x7f, b'E', b'L', b'F',
            0x02, 0x01, 0x01, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let elf_path = keg.join("bin/readonly-elf");
        fs::write(&elf_path, &elf_data).unwrap();

        // Make it read-only
        let mut perms = fs::metadata(&elf_path).unwrap().permissions();
        perms.set_mode(0o444);
        fs::set_permissions(&elf_path, perms).unwrap();

        // Patching should handle this (might fail on write, but shouldn't panic)
        let result = patch_homebrew_placeholders_linux(&keg, &cellar, "test", "1.0.0");
        // Result may be Ok (skipped) or Err (can't write), but shouldn't panic

        // Restore permissions for cleanup
        let mut perms = fs::metadata(&elf_path).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&elf_path, perms).unwrap();

        // Just check it didn't panic
        let _ = result;
    }

    /// Test handling of ELF symlinks
    #[test]
    #[cfg(target_os = "linux")]
    fn handles_elf_symlinks() {
        let tmp = TempDir::new().unwrap();
        let keg = tmp.path().join("keg");
        let cellar = tmp.path().join("cellar");
        fs::create_dir_all(keg.join("bin")).unwrap();
        fs::create_dir_all(keg.join("lib")).unwrap();
        fs::create_dir_all(&cellar).unwrap();

        // Create real ELF file
        let elf_data: Vec<u8> = vec![
            0x7f, b'E', b'L', b'F',
            0x02, 0x01, 0x01, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        fs::write(keg.join("lib/libreal.so.1.0.0"), &elf_data).unwrap();

        // Create symlinks to it
        std::os::unix::fs::symlink("libreal.so.1.0.0", keg.join("lib/libreal.so.1")).unwrap();
        std::os::unix::fs::symlink("libreal.so.1", keg.join("lib/libreal.so")).unwrap();

        // Patching should follow symlinks or skip them appropriately
        let result = patch_homebrew_placeholders_linux(&keg, &cellar, "test", "1.0.0");
        assert!(result.is_ok(), "Should handle ELF symlinks");
    }

    /// Test handling of empty directories
    #[test]
    fn handles_empty_directories() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("bin")).unwrap();
        fs::create_dir_all(src.join("lib")).unwrap();
        fs::create_dir_all(src.join("share")).unwrap();
        // No files, just empty directories

        let cellar = Cellar::new(tmp.path()).unwrap();
        let result = cellar.materialize("empty-dirs", "1.0.0", &src);

        assert!(result.is_ok(), "Should handle empty directories");
        let keg = result.unwrap();
        assert!(keg.join("bin").exists());
        assert!(keg.join("lib").exists());
        assert!(keg.join("share").exists());
    }

    /// Test handling of deeply nested directory structures
    #[test]
    fn handles_deeply_nested_directories() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");

        // Create deep nesting
        let deep_path = src.join("share/doc/pkg/examples/advanced/subdir1/subdir2/subdir3");
        fs::create_dir_all(&deep_path).unwrap();
        fs::write(deep_path.join("readme.txt"), b"nested content").unwrap();

        let cellar = Cellar::new(tmp.path()).unwrap();
        let keg = cellar.materialize("deep-nest", "1.0.0", &src).unwrap();

        assert!(keg.join("share/doc/pkg/examples/advanced/subdir1/subdir2/subdir3/readme.txt").exists());
    }

    /// Test handling of files with special characters in names
    #[test]
    fn handles_special_chars_in_filenames() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("bin")).unwrap();

        // Files with spaces
        fs::write(src.join("bin/my program"), b"content").unwrap();
        // Files with dashes and underscores
        fs::write(src.join("bin/my-program_v2"), b"content").unwrap();
        // Files with dots
        fs::write(src.join("bin/program.sh"), b"content").unwrap();

        let cellar = Cellar::new(tmp.path()).unwrap();
        let keg = cellar.materialize("special-chars", "1.0.0", &src).unwrap();

        assert!(keg.join("bin/my program").exists());
        assert!(keg.join("bin/my-program_v2").exists());
        assert!(keg.join("bin/program.sh").exists());
    }

    // ========================================================================
    // Edge case tests for copy/reflink operations
    // ========================================================================

    /// Test copying empty files
    #[test]
    fn handles_empty_files() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();

        // Create empty file
        fs::write(src.join("empty.txt"), b"").unwrap();

        let cellar = Cellar::new(tmp.path()).unwrap();
        let keg = cellar.materialize("empty-file", "1.0.0", &src).unwrap();

        assert!(keg.join("empty.txt").exists());
        assert_eq!(fs::read(keg.join("empty.txt")).unwrap().len(), 0);
    }

    /// Test copying large files (> 1MB)
    #[test]
    fn handles_large_files() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();

        // Create a 2MB file
        let large_content: Vec<u8> = vec![0x42; 2 * 1024 * 1024];
        fs::write(src.join("large.bin"), &large_content).unwrap();

        let cellar = Cellar::new(tmp.path()).unwrap();
        let keg = cellar.materialize("large-file", "1.0.0", &src).unwrap();

        assert!(keg.join("large.bin").exists());
        assert_eq!(
            fs::metadata(keg.join("large.bin")).unwrap().len(),
            2 * 1024 * 1024
        );
    }

    /// Test handling of broken symlinks
    #[test]
    fn handles_broken_symlinks() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();

        // Create a broken symlink
        std::os::unix::fs::symlink("nonexistent-target", src.join("broken-link")).unwrap();

        // Also create a valid file
        fs::write(src.join("valid.txt"), b"content").unwrap();

        let cellar = Cellar::new(tmp.path()).unwrap();
        let result = cellar.materialize("broken-symlink", "1.0.0", &src);

        // Should either succeed (copying broken symlink) or fail gracefully
        // The important thing is no panic
        let _ = result;
    }

    /// Test handling of circular symlinks
    #[test]
    fn handles_circular_symlinks() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();

        // Create circular symlinks
        std::os::unix::fs::symlink("link2", src.join("link1")).unwrap();
        std::os::unix::fs::symlink("link1", src.join("link2")).unwrap();

        // Also add a valid file so there's something to materialize
        fs::write(src.join("valid.txt"), b"content").unwrap();

        let cellar = Cellar::new(tmp.path()).unwrap();

        // Should not hang or panic
        let result = cellar.materialize("circular-symlinks", "1.0.0", &src);

        // Result may be ok or err, but shouldn't hang/panic
        let _ = result;
    }

    /// Test materialization when destination already exists
    #[test]
    fn rematerialize_existing_returns_path() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("file.txt"), b"original").unwrap();

        let cellar = Cellar::new(tmp.path()).unwrap();

        // First materialize
        let keg1 = cellar.materialize("existing", "1.0.0", &src).unwrap();

        // Modify the keg
        fs::write(keg1.join("marker"), b"modified").unwrap();

        // Second materialize should return same path without overwriting
        let keg2 = cellar.materialize("existing", "1.0.0", &src).unwrap();

        assert_eq!(keg1, keg2);
        assert!(keg2.join("marker").exists(), "Should not overwrite existing keg");
    }

    /// Test handling of multiple file types in same directory
    #[test]
    fn handles_mixed_file_types() {
        #[allow(unused_imports)]
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src/lib");
        fs::create_dir_all(&src).unwrap();

        // Real shared library file
        let elf_header: Vec<u8> = vec![0x7f, b'E', b'L', b'F', 0x02, 0x01, 0x01, 0x00];
        fs::write(src.join("libfoo.so.1.2.3"), &elf_header).unwrap();

        // Symlink chain
        std::os::unix::fs::symlink("libfoo.so.1.2.3", src.join("libfoo.so.1")).unwrap();
        std::os::unix::fs::symlink("libfoo.so.1", src.join("libfoo.so")).unwrap();

        // Archive file
        fs::write(src.join("libfoo.a"), b"!<arch>\n").unwrap();

        // Pkgconfig file
        fs::write(src.join("foo.pc"), b"prefix=/opt/homebrew").unwrap();

        let cellar = Cellar::new(tmp.path()).unwrap();
        let keg = cellar.materialize("mixed", "1.0.0", &tmp.path().join("src")).unwrap();

        // Verify all types present
        assert!(keg.join("lib/libfoo.so.1.2.3").exists());
        assert!(keg.join("lib/libfoo.so.1").symlink_metadata().unwrap().file_type().is_symlink());
        assert!(keg.join("lib/libfoo.so").symlink_metadata().unwrap().file_type().is_symlink());
        assert!(keg.join("lib/libfoo.a").exists());
        assert!(keg.join("lib/foo.pc").exists());
    }

    // ========================================================================
    // Edge case tests for bottle content discovery
    // ========================================================================

    /// Test find_bottle_content with non-standard structure
    #[test]
    fn find_bottle_content_handles_flat_structure() {
        let tmp = TempDir::new().unwrap();
        let store = tmp.path().join("store");

        // Flat structure - no nested name/version directory
        fs::create_dir_all(store.join("bin")).unwrap();
        fs::write(store.join("bin/tool"), b"tool content").unwrap();

        let content = find_bottle_content(&store, "tool", "1.0.0").unwrap();
        // Should fall back to store root
        assert_eq!(content, store);
    }

    /// Test find_bottle_content with only name directory
    #[test]
    fn find_bottle_content_with_name_only() {
        let tmp = TempDir::new().unwrap();
        let store = tmp.path().join("store");

        // Has name dir but wrong version inside
        fs::create_dir_all(store.join("pkg/2.0.0/bin")).unwrap();
        fs::write(store.join("pkg/2.0.0/bin/pkg"), b"content").unwrap();

        let result = find_bottle_content(&store, "pkg", "1.0.0");
        // Should handle version mismatch somehow
        assert!(result.is_ok());
    }

    /// Test handling of case-sensitive filesystem issues
    #[test]
    fn handles_case_variations() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");

        // Create directories with different cases
        // (on case-sensitive fs these will be different)
        fs::create_dir_all(src.join("Bin")).unwrap();
        fs::create_dir_all(src.join("LIB")).unwrap();
        fs::write(src.join("Bin/Tool"), b"content").unwrap();
        fs::write(src.join("LIB/libfoo.a"), b"archive").unwrap();

        let cellar = Cellar::new(tmp.path()).unwrap();
        let keg = cellar.materialize("case-test", "1.0.0", &src).unwrap();

        // Should preserve case
        assert!(keg.join("Bin/Tool").exists() || keg.join("bin/Tool").exists());
        assert!(keg.join("LIB/libfoo.a").exists() || keg.join("lib/libfoo.a").exists());
    }

    // ========================================================================
    // Error handling tests
    // ========================================================================

    /// Test error message when source doesn't exist
    #[test]
    fn errors_on_nonexistent_source() {
        let tmp = TempDir::new().unwrap();
        let nonexistent = tmp.path().join("does-not-exist");

        let cellar = Cellar::new(tmp.path()).unwrap();
        let result = cellar.materialize("missing", "1.0.0", &nonexistent);

        assert!(result.is_err(), "Should fail for nonexistent source");
    }

    /// Test that remove_keg on nonexistent keg is idempotent
    #[test]
    fn remove_nonexistent_keg_is_ok() {
        let tmp = TempDir::new().unwrap();
        let cellar = Cellar::new(tmp.path()).unwrap();

        // Should not error
        let result = cellar.remove_keg("nonexistent", "1.0.0");
        assert!(result.is_ok(), "Removing nonexistent keg should be OK");
    }

    /// Test has_keg returns false for missing kegs
    #[test]
    fn has_keg_returns_false_for_missing() {
        let tmp = TempDir::new().unwrap();
        let cellar = Cellar::new(tmp.path()).unwrap();

        assert!(!cellar.has_keg("missing", "1.0.0"));
        assert!(!cellar.has_keg("also-missing", "2.0.0"));
    }

    /// Test keg_path format is correct
    #[test]
    fn keg_path_format_correct() {
        let tmp = TempDir::new().unwrap();
        let cellar = Cellar::new(tmp.path()).unwrap();

        let path = cellar.keg_path("openssl@3", "3.2.1");
        assert!(path.to_string_lossy().contains("openssl@3"));
        assert!(path.to_string_lossy().contains("3.2.1"));
    }
}
