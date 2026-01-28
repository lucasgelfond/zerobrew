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

/// Patch @@HOMEBREW_CELLAR@@ and @@HOMEBREW_PREFIX@@ placeholders in ELF binaries.
/// Uses `elb` crate to natively update RPATH and RUNPATH.
fn patch_elf_placeholders(keg_path: &Path, prefix_dir: &Path) -> Result<(), Error> {
    let lib_path = prefix_dir.join("lib").to_string_lossy().to_string();
    let lib_path_c = CString::new(lib_path).map_err(|e| Error::StoreCorruption {
        message: format!("Invalid lib path for CString: {e}"),
    })?;

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
}
