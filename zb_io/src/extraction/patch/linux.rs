use std::fs;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use zb_core::Error;

/// Patch @@HOMEBREW_CELLAR@@ and @@HOMEBREW_PREFIX@@ placeholders in both ELF binaries and text files.
#[cfg(target_os = "linux")]
pub fn patch_placeholders(
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
        "/lib64/ld-linux-x86-64.so.2",     // x86_64
        "/usr/lib64/ld-linux-x86-64.so.2", // x86_64
        "/lib/ld-linux-aarch64.so.1",      // aarch64/ARM64
        "/usr/lib/ld-linux-aarch64.so.1",  // aarch64/ARM64
        "/lib/ld-linux-armhf.so.3",        // ARM hard float
        "/usr/lib/ld-linux-armhf.so.3",    // ARM hard float
        "/lib/ld-linux.so.3",              // ARM
        "/lib/ld-linux.so.2",              // old ARM
        "/lib64/ld64.so.2",                // ppc64
        "/lib64/ld64.so.1",                // s390x
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
/// Uses `arwen` crate to natively update RPATH, RUNPATH, and optionally the ELF interpreter.
fn patch_elf_placeholders(keg_path: &Path, prefix_dir: &Path) -> Result<(), Error> {
    let lib_path = prefix_dir.join("lib").to_string_lossy().to_string();

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
    // Use a dashmap or similar for thread-safe inode tracking if needed,
    // but we can just collect and then process, or use a Mutex.
    let processed_inodes = std::sync::Mutex::new(std::collections::HashSet::new());

    // Clone for use in parallel closure
    let target_interpreter = target_interpreter.clone();
    let old_prefix = "@@HOMEBREW_PREFIX@@";
    let new_prefix = prefix_dir.to_string_lossy().to_string();

    elf_files.par_iter().for_each(|path| {
        // Check hardlinks
        if let Ok(meta) = fs::metadata(path) {
            use std::os::unix::fs::MetadataExt;
            let inode = (meta.dev(), meta.ino());
            let mut inodes = processed_inodes.lock().unwrap();
            if !inodes.insert(inode) {
                return; // Already processed this inode
            }
        }

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
            if let Err(e) = fs::set_permissions(path, perms) {
                eprintln!(
                    "Warning: Failed to make file writable: {}: {}",
                    path.display(),
                    e
                );
                patch_failures.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }

        let result = (|| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let content = fs::read(path)?;
            let mut elf = arwen::elf::ElfContainer::parse(&content)?;

            // Check if it is a dynamic ELF
            let has_dynamic_segment = elf
                .inner
                .builder()
                .segments
                .iter()
                .any(|s| s.p_type == object::elf::PT_DYNAMIC);
            if !has_dynamic_segment {
                return Ok(());
            }

            // Set page size for alignment
            let page_size = elf.get_page_size();
            let _ = elf.set_page_size(page_size);

            // RPATH
            let old_rpaths = elf.get_rpath();
            let mut new_rpaths: Vec<String> = if old_rpaths.is_empty() {
                Vec::new()
            } else {
                old_rpaths
                    .iter()
                    .map(|r| r.replace(old_prefix, &new_prefix))
                    .filter(|r| r.starts_with(&new_prefix) || r.starts_with("$ORIGIN"))
                    .collect()
            };

            if !new_rpaths.contains(&lib_path) {
                new_rpaths.push(lib_path.clone());
            }

            let new_rpath_str = new_rpaths.join(":");
            if !new_rpath_str.is_empty() {
                let _ = elf.set_runpath(&new_rpath_str);
            }

            // Interpreter
            let is_executable = elf.inner.builder().header.e_type == object::elf::ET_EXEC
                || (elf.inner.builder().header.e_type == object::elf::ET_DYN
                    && elf.inner.elf_interpreter().is_some());

            if is_executable && let Some(current_interp_bytes) = elf.inner.elf_interpreter() {
                let current_interp_str = String::from_utf8_lossy(current_interp_bytes);

                let target_interp_path = if current_interp_str.contains(old_prefix) {
                    let expanded = current_interp_str.replace(old_prefix, &new_prefix);
                    let expanded_path = PathBuf::from(&expanded);
                    if expanded_path.exists() {
                        Some(expanded_path)
                    } else {
                        find_system_ld_so()
                    }
                } else {
                    target_interpreter.clone()
                };

                if let Some(target_path) = target_interp_path {
                    let target_str = target_path.to_string_lossy();
                    let _ = elf.set_interpreter(&target_str);
                }
            }

            // Atomic write
            let temp_path = path.with_extension("tmp_patch");
            {
                let mut temp_file = fs::File::create(&temp_path)?;
                elf.write(&mut temp_file)?;
            }
            fs::rename(temp_path, path)?;

            // Restore original permissions (including execute bit) after atomic write
            let mut perms = metadata.permissions();
            perms.set_mode(original_mode);
            fs::set_permissions(path, perms)?;

            Ok(())
        })();

        if let Err(e) = result {
            eprintln!("Warning: Failed to patch ELF at {}: {}", path.display(), e);
            patch_failures.fetch_add(1, Ordering::Relaxed);
        }
    });

    let failures = patch_failures.load(Ordering::Relaxed);
    if failures > 0 {
        eprintln!(
            "Warning: Failed to patch {failures} ELF files. These packages may not work correctly until manually patched."
        );
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

            if !content.contains("@@HOMEBREW_") {
                return Ok(());
            }

            let new_content = content
                .replace("@@HOMEBREW_PREFIX@@", &prefix_str)
                .replace("@@HOMEBREW_CELLAR@@", &cellar_str)
                .replace("@@HOMEBREW_REPOSITORY@@", &prefix_str)
                .replace("@@HOMEBREW_LIBRARY@@", &format!("{}/Library", prefix_str))
                .replace("@@HOMEBREW_PERL@@", "/usr/bin/perl")
                .replace("@@HOMEBREW_JAVA@@", "/usr/bin/java");

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
            .arg("-Wl,-rpath,@@HOMEBREW_PREFIX@@/lib")
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
            "#!/bin/bash\necho @@HOMEBREW_PREFIX@@\necho @@HOMEBREW_CELLAR@@\necho @@HOMEBREW_LIBRARY@@\necho @@HOMEBREW_PERL@@",
        )
        .unwrap();

        let result = patch_placeholders(&pkg_dir, &prefix, "testpkg", "1.0.0");
        assert!(result.is_ok());

        let content = fs::read_to_string(&script_path).unwrap();
        assert!(content.contains(prefix.to_str().unwrap()));
        assert!(content.contains(cellar.to_str().unwrap()));
        assert!(content.contains(&format!("{}/Library", prefix.to_str().unwrap())));
        assert!(content.contains("/usr/bin/perl"));
        assert!(!content.contains("@@HOMEBREW_"));
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

        // Record original permissions (should include execute bit from cc)
        let original_mode = fs::metadata(&elf_path).unwrap().permissions().mode();
        assert!(
            original_mode & 0o111 != 0,
            "compiled binary should be executable"
        );

        let result = patch_placeholders(&pkg_dir, &prefix, "testpkg", "1.0.0");
        assert!(result.is_ok());

        // Verify permissions are preserved after patching
        let new_mode = fs::metadata(&elf_path).unwrap().permissions().mode();
        assert_eq!(
            original_mode & 0o777,
            new_mode & 0o777,
            "permissions should be preserved after patching"
        );
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
