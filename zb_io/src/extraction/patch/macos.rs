use std::fs;
use std::path::{Path, PathBuf};
use zb_core::Error;

const HOMEBREW_PREFIXES: &[&str] = &[
    "/opt/homebrew",
    "/usr/local/Homebrew",
    "/usr/local",
    "/home/linuxbrew/.linuxbrew",
];

/// Patch hardcoded Homebrew paths in text files.
fn patch_text_file_strings(path: &Path, new_prefix: &str, new_cellar: &str) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;

    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Ok(()),
    };

    let mut buf = [0u8; 8192];
    let n = match std::io::Read::read(&mut file, &mut buf) {
        Ok(n) => n,
        Err(_) => return Ok(()),
    };

    if buf[..n].contains(&0) {
        return Ok(());
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    if !content.contains("@@HOMEBREW_")
        && !content.contains("/opt/homebrew")
        && !content.contains("/usr/local")
        && !content.contains("/home/linuxbrew")
    {
        return Ok(());
    }

    let mut new_content = content.clone();
    let mut changed = false;

    new_content = new_content
        .replace("@@HOMEBREW_PREFIX@@", new_prefix)
        .replace("@@HOMEBREW_CELLAR@@", new_cellar)
        .replace("@@HOMEBREW_REPOSITORY@@", new_prefix)
        .replace("@@HOMEBREW_LIBRARY@@", &format!("{}/Library", new_prefix))
        .replace("@@HOMEBREW_PERL@@", "/usr/bin/perl")
        .replace("@@HOMEBREW_JAVA@@", "/usr/bin/java");

    if new_content != content {
        changed = true;
    }

    for old_prefix in HOMEBREW_PREFIXES {
        if old_prefix == &new_prefix {
            continue;
        }
        let replaced = new_content.replace(old_prefix, new_prefix);
        if replaced != new_content {
            new_content = replaced;
            changed = true;
        }
    }

    if !changed {
        return Ok(());
    }

    let metadata = fs::metadata(path).map_err(|e| Error::StoreCorruption {
        message: format!("failed to read metadata: {e}"),
    })?;
    let original_mode = metadata.permissions().mode();
    let is_readonly = original_mode & 0o200 == 0;

    if is_readonly {
        let mut perms = metadata.permissions();
        perms.set_mode(original_mode | 0o200);
        fs::set_permissions(path, perms).map_err(|e| Error::StoreCorruption {
            message: format!("failed to make writable: {e}"),
        })?;
    }

    fs::write(path, new_content).map_err(|e| Error::StoreCorruption {
        message: format!("failed to write file: {e}"),
    })?;

    if is_readonly {
        let mut perms = metadata.permissions();
        perms.set_mode(original_mode);
        fs::set_permissions(path, perms).map_err(|e| Error::StoreCorruption {
            message: format!("failed to restore permissions: {e}"),
        })?;
    }

    Ok(())
}

/// Patch hardcoded Homebrew paths in Mach-O binary data sections.
/// This handles paths like /opt/homebrew/opt/git/libexec/git-core that are baked into binaries.
fn patch_macho_binary_strings(path: &Path, new_prefix: &str) -> Result<(), Error> {
    use std::io::{Read as _, Write as _};
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path).map_err(|e| Error::StoreCorruption {
        message: format!("failed to read metadata: {e}"),
    })?;
    let original_mode = metadata.permissions().mode();
    let is_readonly = original_mode & 0o200 == 0;

    if is_readonly {
        let mut perms = metadata.permissions();
        perms.set_mode(original_mode | 0o200);
        fs::set_permissions(path, perms).map_err(|e| Error::StoreCorruption {
            message: format!("failed to make writable: {e}"),
        })?;
    }

    let mut file = fs::File::open(path).map_err(|e| Error::StoreCorruption {
        message: format!("failed to open file: {e}"),
    })?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)
        .map_err(|e| Error::StoreCorruption {
            message: format!("failed to read file: {e}"),
        })?;
    drop(file);

    let original_contents = contents.clone();
    let mut patched = false;

    for old_prefix in HOMEBREW_PREFIXES {
        if old_prefix == &new_prefix {
            continue;
        }

        let old_bytes = old_prefix.as_bytes();
        let new_bytes = new_prefix.as_bytes();

        if new_bytes.len() > old_bytes.len() {
            continue;
        }

        let mut i = 0;
        while i < contents.len() {
            if i + old_bytes.len() > contents.len() {
                break;
            }

            if contents[i..i + old_bytes.len()] == *old_bytes {
                let next = contents.get(i + old_bytes.len()).copied();
                let is_path_boundary = matches!(next, None | Some(0) | Some(b'/'));

                if is_path_boundary {
                    contents[i..i + new_bytes.len()].copy_from_slice(new_bytes);

                    if new_bytes.len() < old_bytes.len() {
                        for j in i + new_bytes.len()..i + old_bytes.len() {
                            contents[j] = 0;
                        }
                    }

                    patched = true;
                }
            }
            i += 1;
        }
    }

    if patched && contents != original_contents {
        let temp_path = path.with_extension("tmp_patch");
        let mut temp_file = fs::File::create(&temp_path).map_err(|e| Error::StoreCorruption {
            message: format!("failed to create temp file: {e}"),
        })?;
        temp_file
            .write_all(&contents)
            .map_err(|e| Error::StoreCorruption {
                message: format!("failed to write temp file: {e}"),
            })?;
        drop(temp_file);

        fs::rename(&temp_path, path).map_err(|e| Error::StoreCorruption {
            message: format!("failed to rename temp file: {e}"),
        })?;

        match std::process::Command::new("codesign")
            .args(["--force", "--sign", "-", &path.to_string_lossy()])
            .output()
        {
            Ok(output) if !output.status.success() => {
                eprintln!(
                    "Warning: Failed to re-sign {}: {}",
                    path.display(),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Err(e) => {
                eprintln!(
                    "Warning: Failed to execute codesign for {}: {}",
                    path.display(),
                    e
                );
            }
            _ => {}
        }
    }

    if is_readonly {
        let mut perms = metadata.permissions();
        perms.set_mode(original_mode);
        let _ = fs::set_permissions(path, perms);
    }

    Ok(())
}

/// Patch @@HOMEBREW_CELLAR@@ and @@HOMEBREW_PREFIX@@ placeholders in Mach-O binaries.
/// Also fixes version mismatches where a bottle references a different version of itself.
/// Additionally patches hardcoded Homebrew paths in binary data sections and text files.
/// Uses rayon for parallel processing.
pub fn patch_homebrew_placeholders(
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

    let patch_failures = AtomicUsize::new(0);

    // First pass: patch binary strings in Mach-O files
    macho_files.par_iter().for_each(|path| {
        if patch_macho_binary_strings(path, &prefix_str).is_err() {
            patch_failures.fetch_add(1, Ordering::Relaxed);
        }
    });

    // Second pass: patch text files
    let text_files: Vec<PathBuf> = walkdir::WalkDir::new(keg_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();

    text_files.par_iter().for_each(|path| {
        let _ = patch_text_file_strings(path, &prefix_str, &cellar_str);
    });

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

    // Third pass: Process Mach-O files for install_name_tool patching
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
pub fn codesign_and_strip_xattrs(keg_path: &Path) -> Result<(), Error> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_patch_macho_binary_strings() {
        let tmp = TempDir::new().unwrap();
        let test_file = tmp.path().join("test_binary");

        let old_prefix = "/home/linuxbrew/.linuxbrew";
        let new_prefix = "/opt/zerobrew/prefix";

        let mut contents = Vec::new();
        contents.extend_from_slice(b"\xfe\xed\xfa\xcf");
        contents.extend_from_slice(b"some random data\0");
        contents.extend_from_slice(old_prefix.as_bytes());
        contents.extend_from_slice(b"/opt/git/libexec/git-core\0");
        contents.extend_from_slice(b"more data\0");
        contents.extend_from_slice(old_prefix.as_bytes());
        contents.extend_from_slice(b"/lib/libfoo.dylib\0");
        contents.extend_from_slice(b"end\0");

        fs::write(&test_file, &contents).unwrap();

        let result = patch_macho_binary_strings(&test_file, new_prefix);
        assert!(result.is_ok());

        let patched = fs::read(&test_file).unwrap();
        let patched_str = String::from_utf8_lossy(&patched);

        assert!(patched_str.contains(new_prefix));
        assert!(!patched_str.contains(old_prefix));
    }

    #[test]
    fn test_patch_macho_skips_when_new_prefix_longer() {
        let tmp = TempDir::new().unwrap();
        let test_file = tmp.path().join("test_binary");

        let old_prefix = "/opt/homebrew";
        let new_prefix = "/opt/zerobrew/prefix";

        let mut contents = Vec::new();
        contents.extend_from_slice(b"\xfe\xed\xfa\xcf");
        contents.extend_from_slice(b"some random data\0");
        contents.extend_from_slice(old_prefix.as_bytes());
        contents.extend_from_slice(b"/opt/git/libexec/git-core\0");
        contents.extend_from_slice(b"more data\0");

        let original = contents.clone();
        fs::write(&test_file, &contents).unwrap();

        let result = patch_macho_binary_strings(&test_file, new_prefix);
        assert!(result.is_ok());

        let patched = fs::read(&test_file).unwrap();
        assert_eq!(
            patched, original,
            "binary should be unchanged when new prefix is longer than old"
        );
    }

    #[test]
    fn test_patch_text_file_strings() {
        let tmp = TempDir::new().unwrap();
        let test_file = tmp.path().join("test_script.sh");

        let content = r#"#!/bin/bash
export GIT_EXEC_PATH=/opt/homebrew/opt/git/libexec/git-core
export PREFIX=@@HOMEBREW_PREFIX@@
export CELLAR=@@HOMEBREW_CELLAR@@
export LIBRARY=@@HOMEBREW_LIBRARY@@
export PERL=@@HOMEBREW_PERL@@
echo "Hello from $PREFIX"
"#;

        fs::write(&test_file, content).unwrap();

        let new_prefix = "/opt/zerobrew/prefix";
        let new_cellar = format!("{}/Cellar", new_prefix);

        let result = patch_text_file_strings(&test_file, new_prefix, &new_cellar);
        assert!(result.is_ok());

        let patched = fs::read_to_string(&test_file).unwrap();
        assert!(patched.contains(new_prefix));
        assert!(!patched.contains("/opt/homebrew"));
        assert!(!patched.contains("@@HOMEBREW_"));
        assert!(patched.contains("/opt/zerobrew/prefix/opt/git/libexec/git-core"));
        assert!(patched.contains("/opt/zerobrew/prefix/Cellar"));
        assert!(patched.contains("/opt/zerobrew/prefix/Library"));
        assert!(patched.contains("/usr/bin/perl"));
    }
}
