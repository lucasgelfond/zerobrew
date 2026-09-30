//! Check that the libraries a keg's binaries load are present.
//!
//! A bottle is built against the versions of its dependencies that were
//! current at the time. If an installed dependency is older or newer than
//! that and no longer ships a library the bottle loads, the binary fails at
//! runtime with "Library not loaded". This finds those cases up front.
//!
//! Only libraries inside the zerobrew prefix are checked. System libraries
//! live in the dyld shared cache rather than on disk, and `@rpath`-style
//! references can't be resolved without the loader.
//!
//! Only implemented for Mach-O. On other platforms nothing is reported.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The libraries under `prefix` that binaries in `keg_path` load but that
/// don't exist, sorted and without duplicates.
#[cfg(target_os = "macos")]
pub fn missing_libraries(keg_path: &Path, prefix: &Path) -> Vec<PathBuf> {
    use rayon::prelude::*;
    use std::process::Command;

    use crate::extraction::patch::macos::is_macho;

    let binaries: Vec<PathBuf> = walkdir::WalkDir::new(keg_path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && is_macho(entry.path()))
        .map(walkdir::DirEntry::into_path)
        .collect();

    let missing: BTreeSet<PathBuf> = binaries
        .par_iter()
        .flat_map_iter(|binary| {
            let output = Command::new("otool").arg("-L").arg(binary).output();
            let libraries = match output {
                Ok(output) if output.status.success() => {
                    linked_libraries(&String::from_utf8_lossy(&output.stdout))
                }
                _ => Vec::new(),
            };
            libraries
                .into_iter()
                .filter(|library| library.starts_with(prefix) && !library.exists())
        })
        .collect();

    missing.into_iter().collect()
}

#[cfg(not(target_os = "macos"))]
pub fn missing_libraries(_keg_path: &Path, _prefix: &Path) -> Vec<PathBuf> {
    Vec::new()
}

/// The absolute library paths listed by `otool -L`.
///
/// Each library is on an indented line, followed by its versions in
/// parentheses. Unindented lines name the file (and, for universal binaries,
/// the architecture) being described.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn linked_libraries(otool_output: &str) -> Vec<PathBuf> {
    let libraries: BTreeSet<PathBuf> = otool_output
        .lines()
        .filter(|line| line.starts_with(char::is_whitespace))
        .map(|line| {
            let line = line.trim();
            line.rfind(" (compatibility version")
                .map_or(line, |end| &line[..end])
        })
        .filter(|library| library.starts_with('/'))
        .map(PathBuf::from)
        .collect();
    libraries.into_iter().collect()
}

/// The package a library under `prefix/opt/<name>/` belongs to.
pub fn library_owner(library: &Path, prefix: &Path) -> Option<String> {
    library
        .strip_prefix(prefix.join("opt"))
        .ok()?
        .components()
        .next()?
        .as_os_str()
        .to_str()
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_libraries_reads_indented_absolute_paths() {
        let output = "\
/opt/zerobrew/Cellar/nmap/7.95/bin/nmap:
\t/opt/zerobrew/opt/openssl@3/lib/libssl.3.dylib (compatibility version 3.0.0, current version 3.0.0)
\t/opt/zerobrew/opt/libssh2/lib/libssh2.1.dylib (compatibility version 2.0.0, current version 2.1.0)
\t@rpath/libfoo.dylib (compatibility version 1.0.0, current version 1.0.0)
\t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1351.0.0)
";

        assert_eq!(
            linked_libraries(output),
            [
                PathBuf::from("/opt/zerobrew/opt/libssh2/lib/libssh2.1.dylib"),
                PathBuf::from("/opt/zerobrew/opt/openssl@3/lib/libssl.3.dylib"),
                PathBuf::from("/usr/lib/libSystem.B.dylib"),
            ]
        );
    }

    #[test]
    fn linked_libraries_merges_universal_binary_architectures() {
        let output = "\
/opt/zerobrew/Cellar/foo/1.0/bin/foo (architecture x86_64):
\t/opt/zerobrew/opt/bar/lib/libbar.2.dylib (compatibility version 2.0.0, current version 2.0.0)
/opt/zerobrew/Cellar/foo/1.0/bin/foo (architecture arm64):
\t/opt/zerobrew/opt/bar/lib/libbar.2.dylib (compatibility version 2.0.0, current version 2.0.0)
";

        assert_eq!(
            linked_libraries(output),
            [PathBuf::from("/opt/zerobrew/opt/bar/lib/libbar.2.dylib")]
        );
    }

    #[test]
    fn library_owner_is_the_opt_directory() {
        let prefix = Path::new("/opt/zerobrew");

        assert_eq!(
            library_owner(
                Path::new("/opt/zerobrew/opt/openssl@3/lib/libssl.3.dylib"),
                prefix
            )
            .as_deref(),
            Some("openssl@3")
        );
        assert_eq!(
            library_owner(Path::new("/opt/zerobrew/lib/libssl.3.dylib"), prefix),
            None
        );
    }

    /// Build a real binary against a real library, then take the library
    /// away, like a dependency upgrade that drops an old library version.
    #[cfg(target_os = "macos")]
    #[test]
    fn missing_libraries_finds_a_removed_dependency_library() {
        use std::fs;
        use std::process::Command;

        let tmp = tempfile::TempDir::new().unwrap();
        let prefix = tmp.path().canonicalize().unwrap();
        let dep_lib = prefix.join("Cellar/dep/1.0/lib");
        let keg = prefix.join("Cellar/app/1.0");
        fs::create_dir_all(&dep_lib).unwrap();
        fs::create_dir_all(keg.join("bin")).unwrap();
        fs::create_dir_all(prefix.join("opt")).unwrap();
        std::os::unix::fs::symlink(prefix.join("Cellar/dep/1.0"), prefix.join("opt/dep")).unwrap();

        let dep_src = prefix.join("dep.c");
        let app_src = prefix.join("app.c");
        fs::write(&dep_src, "int dep(void) { return 1; }\n").unwrap();
        fs::write(
            &app_src,
            "int dep(void); int main(void) { return dep(); }\n",
        )
        .unwrap();

        let dylib = prefix.join("opt/dep/lib/libdep.1.dylib");
        let built = Command::new("cc")
            .args(["-dynamiclib", "-o"])
            .arg(&dylib)
            .arg("-install_name")
            .arg(&dylib)
            .arg(&dep_src)
            .status()
            .is_ok_and(|s| s.success())
            && Command::new("cc")
                .arg("-o")
                .arg(keg.join("bin/app"))
                .arg(&app_src)
                .arg(&dylib)
                .status()
                .is_ok_and(|s| s.success());
        if !built {
            eprintln!("skipping: no working C compiler");
            return;
        }

        assert!(missing_libraries(&keg, &prefix).is_empty());

        fs::remove_file(&dylib).unwrap();

        assert_eq!(missing_libraries(&keg, &prefix), [dylib]);
    }
}
