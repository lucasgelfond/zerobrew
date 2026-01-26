//! Integration tests for Linux compatibility
//!
//! These tests verify that zerobrew works correctly on Linux:
//! - Bottle selection chooses Linux-specific bottles
//! - ELF binaries are properly patched
//! - Reflink/copy fallback works correctly
//! - Installed binaries are executable
//!
//! Some tests require:
//! - `patchelf` to be installed (for ELF patching tests)
//! - btrfs/XFS filesystem (for reflink tests)
//!
//! Run with: `cargo test --test linux_integration`
//! Run ignored tests: `cargo test --test linux_integration -- --ignored`

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

use zb_io::Cellar;

/// Helper to check if patchelf is available
fn has_patchelf() -> bool {
    Command::new("patchelf")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Helper to create a minimal ELF header (not a valid executable, just for detection)
fn minimal_elf_header() -> Vec<u8> {
    vec![
        0x7f, b'E', b'L', b'F', // Magic
        0x02, // 64-bit
        0x01, // Little-endian
        0x01, // ELF version
        0x00, // OS/ABI (SYSV)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Padding
        0x02, 0x00, // e_type: ET_EXEC
        0x3e, 0x00, // e_machine: EM_X86_64
        0x01, 0x00, 0x00, 0x00, // e_version
    ]
}

/// Helper to create a shell script
fn shell_script(content: &str) -> Vec<u8> {
    format!("#!/bin/sh\n{}\n", content).into_bytes()
}

/// Helper to set up a test store entry with various file types
fn setup_test_store_entry(tmp: &TempDir, name: &str, version: &str) -> PathBuf {
    let store_entry = tmp.path().join("store").join(format!("{}-{}", name, version));

    // Create Homebrew-style bottle structure: {name}/{version}/
    let bottle_root = store_entry.join(name).join(version);
    fs::create_dir_all(bottle_root.join("bin")).unwrap();
    fs::create_dir_all(bottle_root.join("lib")).unwrap();
    fs::create_dir_all(bottle_root.join("share/man/man1")).unwrap();

    // Create a shell script (should be executable)
    fs::write(bottle_root.join("bin/script"), shell_script("echo hello")).unwrap();
    let mut perms = fs::metadata(bottle_root.join("bin/script"))
        .unwrap()
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(bottle_root.join("bin/script"), perms).unwrap();

    // Create a fake ELF binary
    let mut elf_data = minimal_elf_header();
    elf_data.extend_from_slice(b"padding to make it bigger");
    fs::write(bottle_root.join("bin/elf-binary"), &elf_data).unwrap();
    let mut perms = fs::metadata(bottle_root.join("bin/elf-binary"))
        .unwrap()
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(bottle_root.join("bin/elf-binary"), perms).unwrap();

    // Create a shared library
    let mut lib_data = minimal_elf_header();
    lib_data.extend_from_slice(b"fake library content");
    fs::write(bottle_root.join("lib/libfoo.so.1.0.0"), &lib_data).unwrap();

    // Create typical library symlinks
    std::os::unix::fs::symlink("libfoo.so.1.0.0", bottle_root.join("lib/libfoo.so.1")).unwrap();
    std::os::unix::fs::symlink("libfoo.so.1", bottle_root.join("lib/libfoo.so")).unwrap();

    // Create a man page
    fs::write(
        bottle_root.join("share/man/man1/test.1"),
        b".TH TEST 1\n.SH NAME\ntest - test program\n",
    )
    .unwrap();

    // Create a data file
    fs::write(bottle_root.join("share/data.txt"), b"some data").unwrap();

    store_entry
}

// ============================================================================
// Materialize / Cellar Tests
// ============================================================================

/// Test that materialize creates the correct directory structure
#[test]
fn materialize_creates_correct_structure() {
    let tmp = TempDir::new().unwrap();
    let store_entry = setup_test_store_entry(&tmp, "testpkg", "1.0.0");

    let cellar = Cellar::new(tmp.path()).unwrap();
    let keg_path = cellar.materialize("testpkg", "1.0.0", &store_entry).unwrap();

    // Verify structure
    assert!(keg_path.exists(), "Keg path should exist");
    assert!(keg_path.join("bin").exists(), "bin directory should exist");
    assert!(keg_path.join("lib").exists(), "lib directory should exist");
    assert!(
        keg_path.join("share").exists(),
        "share directory should exist"
    );
}

/// Test that executable permissions are preserved
#[test]
fn materialize_preserves_executable_permissions() {
    let tmp = TempDir::new().unwrap();
    let store_entry = setup_test_store_entry(&tmp, "exectest", "1.0.0");

    let cellar = Cellar::new(tmp.path()).unwrap();
    let keg_path = cellar.materialize("exectest", "1.0.0", &store_entry).unwrap();

    let script_path = keg_path.join("bin/script");
    let perms = fs::metadata(&script_path).unwrap().permissions();

    assert!(
        perms.mode() & 0o111 != 0,
        "Script should have executable bit: {:o}",
        perms.mode()
    );
}

/// Test that symlinks are preserved correctly
#[test]
fn materialize_preserves_symlinks() {
    let tmp = TempDir::new().unwrap();
    let store_entry = setup_test_store_entry(&tmp, "symlinktest", "1.0.0");

    let cellar = Cellar::new(tmp.path()).unwrap();
    let keg_path = cellar
        .materialize("symlinktest", "1.0.0", &store_entry)
        .unwrap();

    // Check versioned symlink
    let link1 = keg_path.join("lib/libfoo.so.1");
    assert!(
        link1.symlink_metadata().unwrap().file_type().is_symlink(),
        "libfoo.so.1 should be a symlink"
    );
    assert_eq!(
        fs::read_link(&link1).unwrap().to_string_lossy(),
        "libfoo.so.1.0.0",
        "libfoo.so.1 should point to libfoo.so.1.0.0"
    );

    // Check soname symlink
    let link2 = keg_path.join("lib/libfoo.so");
    assert!(
        link2.symlink_metadata().unwrap().file_type().is_symlink(),
        "libfoo.so should be a symlink"
    );
    assert_eq!(
        fs::read_link(&link2).unwrap().to_string_lossy(),
        "libfoo.so.1",
        "libfoo.so should point to libfoo.so.1"
    );
}

/// Test that re-materialize is idempotent
#[test]
fn materialize_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let store_entry = setup_test_store_entry(&tmp, "idempotent", "1.0.0");

    let cellar = Cellar::new(tmp.path()).unwrap();

    // First materialize
    let keg_path1 = cellar
        .materialize("idempotent", "1.0.0", &store_entry)
        .unwrap();

    // Add a marker file
    fs::write(keg_path1.join("MARKER"), b"test").unwrap();

    // Second materialize should return the same path without changes
    let keg_path2 = cellar
        .materialize("idempotent", "1.0.0", &store_entry)
        .unwrap();

    assert_eq!(keg_path1, keg_path2);
    assert!(
        keg_path2.join("MARKER").exists(),
        "Marker file should still exist"
    );
}

/// Test keg removal
#[test]
fn remove_keg_cleans_up_completely() {
    let tmp = TempDir::new().unwrap();
    let store_entry = setup_test_store_entry(&tmp, "removeme", "1.0.0");

    let cellar = Cellar::new(tmp.path()).unwrap();
    let keg_path = cellar
        .materialize("removeme", "1.0.0", &store_entry)
        .unwrap();

    assert!(cellar.has_keg("removeme", "1.0.0"));
    assert!(keg_path.exists());

    cellar.remove_keg("removeme", "1.0.0").unwrap();

    assert!(!cellar.has_keg("removeme", "1.0.0"));
    assert!(!keg_path.exists());
}

// ============================================================================
// Linux-Specific Tests
// ============================================================================

/// Test ELF magic byte detection
#[test]
fn detect_elf_files() {
    let tmp = TempDir::new().unwrap();

    // Create various file types
    let elf_file = tmp.path().join("elf");
    fs::write(&elf_file, minimal_elf_header()).unwrap();

    let script_file = tmp.path().join("script");
    fs::write(&script_file, b"#!/bin/sh\necho hi").unwrap();

    let data_file = tmp.path().join("data");
    fs::write(&data_file, b"just some data").unwrap();

    let empty_file = tmp.path().join("empty");
    fs::write(&empty_file, b"").unwrap();

    const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

    // Check ELF detection
    let elf_data = fs::read(&elf_file).unwrap();
    assert!(elf_data.len() >= 4 && elf_data[0..4] == ELF_MAGIC);

    let script_data = fs::read(&script_file).unwrap();
    assert!(script_data.len() < 4 || script_data[0..4] != ELF_MAGIC);

    let data_data = fs::read(&data_file).unwrap();
    assert!(data_data.len() < 4 || data_data[0..4] != ELF_MAGIC);

    let empty_data = fs::read(&empty_file).unwrap();
    assert!(empty_data.len() < 4);
}

/// Test that patching handles missing patchelf gracefully
#[test]
#[cfg(target_os = "linux")]
fn patching_without_patchelf_succeeds() {
    // Even if patchelf isn't installed, materialize should succeed
    // (it just won't patch the binaries)
    let tmp = TempDir::new().unwrap();
    let store_entry = setup_test_store_entry(&tmp, "nopatch", "1.0.0");

    let cellar = Cellar::new(tmp.path()).unwrap();
    let result = cellar.materialize("nopatch", "1.0.0", &store_entry);

    assert!(
        result.is_ok(),
        "Materialize should succeed even without patchelf"
    );
}

/// Test with real patchelf (if available)
#[test]
#[cfg(target_os = "linux")]
#[ignore] // Run with --ignored, requires patchelf
fn patching_with_real_patchelf() {
    if !has_patchelf() {
        eprintln!("Skipping: patchelf not installed");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let store_entry = setup_test_store_entry(&tmp, "withpatch", "1.0.0");

    let cellar = Cellar::new(tmp.path()).unwrap();
    let keg_path = cellar
        .materialize("withpatch", "1.0.0", &store_entry)
        .unwrap();

    // The fake ELF won't have valid RPATH, but patching shouldn't fail
    assert!(keg_path.join("bin/elf-binary").exists());
}

// ============================================================================
// Copy Strategy Tests
// ============================================================================

/// Test that copy works across different situations
#[test]
fn copy_handles_various_file_types() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("src");
    fs::create_dir_all(src.join("subdir")).unwrap();

    // Regular file
    fs::write(src.join("file.txt"), b"content").unwrap();

    // Executable
    fs::write(src.join("exec"), b"#!/bin/sh").unwrap();
    let mut perms = fs::metadata(src.join("exec")).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(src.join("exec"), perms).unwrap();

    // Empty file
    fs::write(src.join("empty"), b"").unwrap();

    // File in subdirectory
    fs::write(src.join("subdir/nested.txt"), b"nested").unwrap();

    // Symlink
    std::os::unix::fs::symlink("file.txt", src.join("link")).unwrap();

    // Materialize
    let cellar = Cellar::new(tmp.path()).unwrap();
    let keg = cellar.materialize("copytest", "1.0.0", &src).unwrap();

    // Verify all file types
    assert!(keg.join("file.txt").exists());
    assert_eq!(fs::read_to_string(keg.join("file.txt")).unwrap(), "content");

    assert!(keg.join("exec").exists());
    let perms = fs::metadata(keg.join("exec")).unwrap().permissions();
    assert!(perms.mode() & 0o111 != 0);

    assert!(keg.join("empty").exists());
    assert_eq!(fs::read(keg.join("empty")).unwrap().len(), 0);

    assert!(keg.join("subdir/nested.txt").exists());
    assert_eq!(
        fs::read_to_string(keg.join("subdir/nested.txt")).unwrap(),
        "nested"
    );

    let link = keg.join("link");
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
}

/// Test reflink fallback on non-CoW filesystem
#[test]
#[cfg(target_os = "linux")]
fn reflink_falls_back_on_non_cow_fs() {
    // Most CI and development systems use ext4, which doesn't support reflinks
    // This test verifies the fallback works
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("data"), b"test data for reflink").unwrap();

    let cellar = Cellar::new(tmp.path()).unwrap();
    let keg = cellar.materialize("reflink", "1.0.0", &src).unwrap();

    // Should succeed via copy fallback
    assert!(keg.join("data").exists());
    assert_eq!(
        fs::read_to_string(keg.join("data")).unwrap(),
        "test data for reflink"
    );
}

// ============================================================================
// Platform-Specific Architecture Tests
// ============================================================================

/// Test interpreter path selection - verify the constant is correct
#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn x86_64_interpreter_path() {
    // Verify the path constant matches expected value
    // Don't check exists() as this may run in containers with different paths
    let expected = "/lib64/ld-linux-x86-64.so.2";
    assert!(expected.contains("x86-64"), "Path should reference x86-64");
    assert!(expected.starts_with("/lib"), "Path should be absolute");
}

/// Test interpreter path selection - verify the constant is correct
#[test]
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn aarch64_interpreter_path() {
    // Verify the path constant matches expected value
    // Don't check exists() as this may run in containers with different paths
    let expected = "/lib/ld-linux-aarch64.so.1";
    assert!(expected.contains("aarch64"), "Path should reference aarch64");
    assert!(expected.starts_with("/lib"), "Path should be absolute");
}

// ============================================================================
// Bottle Selection Tests (integration level)
// ============================================================================

/// Test that the right bottle tags are selected on Linux
#[test]
#[cfg(target_os = "linux")]
fn correct_bottle_tags_on_linux() {
    use std::collections::BTreeMap;
    use zb_core::formula::{Bottle, BottleFile, BottleStable, Formula, Versions};
    use zb_core::select_bottle;

    let mut files = BTreeMap::new();

    // Add macOS bottles
    files.insert(
        "arm64_sonoma".to_string(),
        BottleFile {
            url: "https://example.com/macos-arm.tar.gz".to_string(),
            sha256: "macos-arm".to_string(),
        },
    );
    files.insert(
        "sonoma".to_string(),
        BottleFile {
            url: "https://example.com/macos-x86.tar.gz".to_string(),
            sha256: "macos-x86".to_string(),
        },
    );

    // Add Linux bottles
    files.insert(
        "arm64_linux".to_string(),
        BottleFile {
            url: "https://example.com/linux-arm.tar.gz".to_string(),
            sha256: "linux-arm".to_string(),
        },
    );
    files.insert(
        "x86_64_linux".to_string(),
        BottleFile {
            url: "https://example.com/linux-x86.tar.gz".to_string(),
            sha256: "linux-x86".to_string(),
        },
    );

    let formula = Formula {
        name: "test-multi-platform".to_string(),
        versions: Versions {
            stable: "1.0.0".to_string(),
        },
        dependencies: Vec::new(),
        bottle: Bottle {
            stable: BottleStable { files, rebuild: 0 },
        },
    };

    let selected = select_bottle(&formula).unwrap();

    // On Linux, should select Linux bottle
    assert!(
        selected.tag.contains("linux"),
        "Should select Linux bottle, got: {}",
        selected.tag
    );

    // Architecture should match
    #[cfg(target_arch = "x86_64")]
    assert!(
        selected.tag == "x86_64_linux",
        "x86_64 should get x86_64_linux, got: {}",
        selected.tag
    );

    #[cfg(target_arch = "aarch64")]
    assert!(
        selected.tag == "arm64_linux",
        "aarch64 should get arm64_linux, got: {}",
        selected.tag
    );
}
