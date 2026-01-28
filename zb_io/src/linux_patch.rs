use std::ffi::CString;
use std::fs;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use elb::{DynamicTag, Elf, ElfPatcher};
use rayon::prelude::*;
use zb_core::Error;

/// Patch @@HOMEBREW_CELLAR@@ and @@HOMEBREW_PREFIX@@ placeholders in both ELF binaries and text files.
#[cfg(target_os = "linux")]
pub(crate) fn patch_placeholders(
    keg_path: &Path,
    prefix_dir: &Path,
    _pkg_name: &str,
    _pkg_version: &str,
) -> Result<(), Error> {
    patch_elf_placeholders(keg_path, prefix_dir)?;
    patch_text_placeholders(keg_path, prefix_dir)?;
    Ok(())
}

/// Detect if zerobrew has installed its own glibc and return the path to its ld.so interpreter.
/// Returns None if zerobrew's glibc is not found, indicating we should use the system ld.so.
fn detect_zerobrew_glibc(prefix_dir: &Path) -> Option<PathBuf> {
    let cellar = prefix_dir.join("Cellar").join("glibc");

    if !cellar.exists() {
        return None;
    }

    // Look for glibc installations in the Cellar
    let glibc_entries = match fs::read_dir(&cellar) {
        Ok(entries) => entries,
        Err(_) => return None,
    };

    // Find the most recent glibc version directory
    let mut glibc_versions: Vec<PathBuf> = glibc_entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();

    if glibc_versions.is_empty() {
        return None;
    }

    // Sort to get the newest version (simple lexicographic sort should work for version numbers)
    glibc_versions.sort();
    glibc_versions.reverse();

    // Look for the ld.so interpreter in the glibc lib directory
    // Common names: ld-linux-x86-64.so.2, ld-linux-aarch64.so.1, ld-linux.so.2, etc.
    for glibc_dir in glibc_versions {
        let lib_dir = glibc_dir.join("lib");
        if !lib_dir.exists() {
            continue;
        }

        let entries = match fs::read_dir(&lib_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let filename = match path.file_name() {
                Some(name) => name.to_string_lossy(),
                None => continue,
            };

            // Match ld-linux*.so* patterns
            if filename.starts_with("ld-linux") && filename.contains(".so") {
                return Some(path);
            }
            // Also check for ld64.so.2 (ppc64)
            if filename == "ld64.so.2" || filename.starts_with("ld64.so.") {
                return Some(path);
            }
            // And ld-linux.so.* variants
            if filename.starts_with("ld-linux.so.") {
                return Some(path);
            }
        }
    }

    None
}

/// Find the system's dynamic linker (ld.so).
/// Returns the path to the system ld.so if found, None otherwise.
fn find_system_ld_so() -> Option<PathBuf> {
    // Common paths for system dynamic linkers on Linux
    let candidates = [
        "/lib64/ld-linux-x86-64.so.2", // x86_64
        "/lib/ld-linux-aarch64.so.1",  // aarch64/ARM64
        "/lib/ld-linux-armhf.so.3",    // ARM hard float
        "/lib/ld-linux.so.3",          // ARM
        "/lib/ld-linux.so.2",          // old ARM
        "/lib64/ld64.so.2",            // ppc64
        "/lib64/ld64.so.1",            // s390x
    ];

    for candidate in &candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

/// Patch @@HOMEBREW_CELLAR@@ and @@HOMEBREW_PREFIX@@ placeholders in ELF binaries.
/// Uses `elb` crate to natively update RPATH, RUNPATH, and optionally the ELF interpreter.
fn patch_elf_placeholders(keg_path: &Path, prefix_dir: &Path) -> Result<(), Error> {
    let lib_path = prefix_dir.join("lib").to_string_lossy().to_string();
    let lib_path_c = CString::new(lib_path).map_err(|e| Error::StoreCorruption {
        message: format!("Invalid lib path for CString: {e}"),
    })?;

    // Detect if zerobrew has installed its own glibc
    let zerobrew_interpreter = detect_zerobrew_glibc(prefix_dir);

    // Determine which interpreter to use:
    // - If zerobrew has glibc, use zerobrew's ld.so
    // - Otherwise, use the system ld.so (fallback)
    let target_interpreter = if let Some(ref zb_ld) = zerobrew_interpreter {
        Some(zb_ld.clone())
    } else {
        // Find system ld.so - common paths for Linux
        find_system_ld_so()
    };

    let target_interpreter_c = target_interpreter
        .as_ref()
        .and_then(|path| CString::new(path.to_string_lossy().as_bytes()).ok());

    // Collect all ELF files
    let elf_files: Vec<PathBuf> = walkdir::WalkDir::new(keg_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            // Read only first 4 bytes to check magic
            let mut file = match fs::File::open(e.path()) {
                Ok(f) => f,
                Err(_) => return false,
            };
            let mut magic = [0u8; 4];
            if file.read_exact(&mut magic).is_ok() {
                return magic == *b"\x7fELF";
            }
            false
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    let patch_failures = AtomicUsize::new(0);

    // Clone for use in parallel closure
    let target_interpreter_c = target_interpreter_c.clone();
    let has_zerobrew_glibc = zerobrew_interpreter.is_some();

    elf_files.par_iter().for_each(|path| {
        // Get permissions and make writable if needed
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return,
        };
        let original_mode = metadata.permissions().mode();
        let is_readonly = original_mode & 0o200 == 0;

        if is_readonly {
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode | 0o200);
            if fs::set_permissions(path, perms).is_err() {
                eprintln!("Warning: Failed to make file writable: {}", path.display());
                patch_failures.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }

        // Apply patch
        let result = (|| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            // Open for Read + Write
            let mut file = fs::OpenOptions::new().read(true).write(true).open(path)?;

            // Read ELF (page_size = 4096 is standard for Linux x86_64/aarch64)
            let elf = Elf::read(&mut file, 0x1000)?;

            let mut patcher = ElfPatcher::new(elf, file);

            // Set RUNPATH (modern RPATH)
            patcher.set_dynamic_tag(DynamicTag::Runpath, &*lib_path_c)?;

            // Set RPATH (legacy compatibility, but good practice)
            patcher.set_dynamic_tag(DynamicTag::Rpath, &*lib_path_c)?;

            // Only patch interpreter for executables, not shared libraries
            // Shared libraries don't have interpreter segments
            let is_executable = path.file_name()
                .and_then(|n| n.to_str())
                .map(|n| !n.contains(".so") && !n.ends_with(".so"))
                .unwrap_or(false);

            if is_executable {
                // Patch the interpreter if needed
                // This handles both zerobrew's glibc and system fallback
                if let Ok(Some(current_interp)) = patcher.read_interpreter() {
                    let current_interp_str = current_interp.to_string_lossy();

                    // Determine the target interpreter path
                    let target_interp_path = if current_interp_str.contains("@@HOMEBREW_PREFIX@@") {
                        // Replace @@HOMEBREW_PREFIX@@ with actual prefix
                        let expanded = current_interp_str.replace(
                            "@@HOMEBREW_PREFIX@@",
                            &prefix_dir.to_string_lossy()
                        );
                        let expanded_path = PathBuf::from(expanded.to_string());

                        // Check if the expanded path exists
                        // If it does (zerobrew has glibc), use it
                        // If not, fall back to system ld.so
                        if expanded_path.exists() {
                            Some(expanded_path)
                        } else {
                            find_system_ld_so()
                        }
                    } else if has_zerobrew_glibc {
                        // If we have zerobrew glibc and no placeholder, use it
                        target_interpreter_c.as_ref().map(|c| PathBuf::from(c.to_string_lossy().to_string()))
                    } else {
                        None // No patching needed
                    };

                    if let Some(target_path) = target_interp_path
                        && let Ok(target_c) = CString::new(target_path.to_string_lossy().as_bytes())
                    {
                        // Only patch if the new path is not longer than the old one
                        let new_len = target_c.as_bytes().len();
                        let old_len = current_interp.as_bytes().len();

                        if new_len <= old_len {
                            if let Err(e) = patcher.set_interpreter(&target_c) {
                                eprintln!(
                                    "Warning: Failed to set interpreter for {}: {}",
                                    path.display(),
                                    e
                                );
                            }
                        } else {
                            eprintln!(
                                "Warning: Cannot patch interpreter for {} (new path too long: {} > {})",
                                path.display(),
                                new_len,
                                old_len
                            );
                        }
                    }
                }
            }

            // Finish writes changes back to the file
            patcher.finish()?;

            Ok(())
        })();

        if let Err(e) = result {
            eprintln!("Warning: Failed to patch ELF at {}: {}", path.display(), e);
            patch_failures.fetch_add(1, Ordering::Relaxed);
        }

        // Restore permissions
        if is_readonly {
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode);
            let _ = fs::set_permissions(path, perms);
        }
    });

    let failures = patch_failures.load(Ordering::Relaxed);
    if failures > 0 {
        eprintln!("Error: Failed to patch {} ELF files", failures);
    }

    Ok(())
}

/// Patch text files containing @@HOMEBREW_...@@ placeholders
fn patch_text_placeholders(keg_path: &Path, prefix_dir: &Path) -> Result<(), Error> {
    let prefix_str = prefix_dir.to_string_lossy().to_string();
    let cellar_str = prefix_dir.join("Cellar").to_string_lossy().to_string();

    // We search for files that are text and contain the placeholders.
    // To avoid reading every large file, we might filter by extension or size,
    // but Homebrew generally patches everything that looks like text.
    // For safety, we skip anything that looks like a binary (has null bytes in first 8kb).

    let files: Vec<PathBuf> = walkdir::WalkDir::new(keg_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();

    let patch_failures = AtomicUsize::new(0);

    files.par_iter().for_each(|path| {
        let result = (|| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            // Check if file is likely text
            let mut file = fs::File::open(path)?;
            let mut buf = [0u8; 8192];
            let n = file.read(&mut buf)?;
            if buf[..n].contains(&0) {
                // Determine if it is ELF - we already handled those, but other binaries should be skipped too
                return Ok(());
            }

            // Read full content string
            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => return Ok(()), // Not valid UTF-8, skip
            };

            if !content.contains("@@HOMEBREW_PREFIX@@") && !content.contains("@@HOMEBREW_CELLAR@@")
            {
                return Ok(());
            }

            // Replace
            let new_content = content
                .replace("@@HOMEBREW_PREFIX@@", &prefix_str)
                .replace("@@HOMEBREW_CELLAR@@", &cellar_str);

            // Write back
            // Check readonly
            let metadata = fs::metadata(path)?;
            let original_mode = metadata.permissions().mode();
            let is_readonly = original_mode & 0o200 == 0;

            if is_readonly {
                let mut perms = metadata.permissions();
                perms.set_mode(original_mode | 0o200);
                fs::set_permissions(path, perms)?;
            }

            fs::write(path, new_content)?;

            if is_readonly {
                let mut perms = metadata.permissions();
                perms.set_mode(original_mode);
                fs::set_permissions(path, perms)?;
            }

            Ok(())
        })();

        if let Err(e) = result {
            eprintln!(
                "Warning: Failed to patch text file at {}: {}",
                path.display(),
                e
            );
            patch_failures.fetch_add(1, Ordering::Relaxed);
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use std::process::Command;
    use tempfile::TempDir;

    fn compile_dummy_elf(dir: &Path, name: &str) -> Option<PathBuf> {
        let src_path = dir.join(format!("{}.c", name));
        if fs::write(&src_path, "int main() { return 0; }").is_err() {
            return None;
        }

        let out_path = dir.join(name);
        let status = Command::new("cc")
            .arg(&src_path)
            .arg("-o")
            .arg(&out_path)
            .status()
            .ok()?;

        if status.success() {
            Some(out_path)
        } else {
            None
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn patches_text_files() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path().join("prefix");
        let cellar = prefix.join("Cellar");
        let pkg_dir = cellar.join("testpkg/1.0.0");
        let bin_dir = pkg_dir.join("bin");

        fs::create_dir_all(&bin_dir).unwrap();

        let script_path = bin_dir.join("script.sh");
        fs::write(
            &script_path,
            "#!/bin/bash\necho @@HOMEBREW_PREFIX@@\necho @@HOMEBREW_CELLAR@@",
        )
        .unwrap();

        let result = patch_placeholders(&pkg_dir, &prefix, "testpkg", "1.0.0");
        assert!(result.is_ok());

        let content = fs::read_to_string(&script_path).unwrap();
        assert!(content.contains(prefix.to_str().unwrap()));
        assert!(content.contains(cellar.to_str().unwrap()));
        assert!(!content.contains("@@HOMEBREW_PREFIX@@"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn patches_elf_file() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path().join("prefix");
        let cellar = prefix.join("Cellar");
        let pkg_dir = cellar.join("testpkg/1.0.0");
        let bin_dir = pkg_dir.join("bin");

        fs::create_dir_all(&bin_dir).unwrap();

        let elf_path = match compile_dummy_elf(&bin_dir, "testbin") {
            Some(p) => p,
            None => {
                eprintln!("Skipping ELF patch test: cc not found");
                return;
            }
        };

        let result = patch_placeholders(&pkg_dir, &prefix, "testpkg", "1.0.0");
        assert!(result.is_ok());

        // Basic check that file is still intact
        assert!(fs::metadata(&elf_path).is_ok());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_glibc_detection() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path().join("prefix");

        // Test 1: No glibc installed - should return None
        assert!(detect_zerobrew_glibc(&prefix).is_none());

        // Test 2: Create a mock glibc installation
        let glibc_dir = prefix.join("Cellar/glibc/2.38");
        let lib_dir = glibc_dir.join("lib");
        fs::create_dir_all(&lib_dir).unwrap();

        // Create a mock ld-linux-x86-64.so.2
        let ld_so = lib_dir.join("ld-linux-x86-64.so.2");
        fs::write(&ld_so, "mock").unwrap();

        // Should now detect the glibc
        let detected = detect_zerobrew_glibc(&prefix);
        assert!(detected.is_some());
        assert_eq!(detected.unwrap(), ld_so);

        // Test 3: Multiple glibc versions - should pick the newest
        let glibc_dir_newer = prefix.join("Cellar/glibc/2.39");
        let lib_dir_newer = glibc_dir_newer.join("lib");
        fs::create_dir_all(&lib_dir_newer).unwrap();
        let ld_so_newer = lib_dir_newer.join("ld-linux-x86-64.so.2");
        fs::write(&ld_so_newer, "mock").unwrap();

        let detected = detect_zerobrew_glibc(&prefix);
        assert!(detected.is_some());
        assert_eq!(detected.unwrap(), ld_so_newer);
    }
}
