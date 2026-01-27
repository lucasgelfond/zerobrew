//! Security tests for migration that attempt exploitation via malicious Tab files.

use std::fs;

use tempfile::TempDir;

use crate::scanner::HomebrewScanner;
use crate::tab::HomebrewTab;

// ============================================================================
// MALICIOUS TAB FILES
// ============================================================================

#[test]
fn attack_tab_with_path_traversal_in_dependency() {
    // ATTACK: Tab file with path traversal in dependency name
    let malicious_tab = r#"{
            "homebrew_version": "4.0.0",
            "installed_on_request": true,
            "runtime_dependencies": [
                {"full_name": "../../../../../../etc/passwd", "version": "1.0.0"}
            ]
        }"#;

    let tab = HomebrewTab::parse(malicious_tab).unwrap();

    // Dependency name parsed as-is
    let deps = tab.runtime_dependencies.as_ref().unwrap();
    assert_eq!(deps[0].full_name, "../../../../../../etc/passwd");

    // This dependency name would be used in:
    // 1. API lookups (won't find it)
    // 2. Path construction (could cause traversal)

    // TODO: Validate dependency names in migration
}

#[test]
fn attack_tab_with_malicious_tap_name() {
    // ATTACK: Custom tap with path traversal
    let malicious_tab = r#"{
            "homebrew_version": "4.0.0",
            "source": {
                "tap": "../../../../../../tmp/evil"
            }
        }"#;

    let tab = HomebrewTab::parse(malicious_tab).unwrap();

    assert_eq!(tab.tap(), Some("../../../../../../tmp/evil"));
    assert!(!tab.is_core_formula());

    // Migrator marks non-core formulas as incompatible
    // So this is already mitigated
    // But demonstrates no tap name validation
}

#[test]
fn attack_tab_with_huge_dependency_list() {
    // ATTACK: Tab with thousands of dependencies (memory DOS)
    let mut deps = Vec::new();
    for i in 0..10_000 {
        deps.push(format!(
            r#"{{"full_name": "dep{}", "version": "1.0.0"}}"#,
            i
        ));
    }

    let malicious_tab = format!(
        r#"{{
                "homebrew_version": "4.0.0",
                "runtime_dependencies": [{}]
            }}"#,
        deps.join(",")
    );

    let start = std::time::Instant::now();
    let result = HomebrewTab::parse(&malicious_tab);
    let elapsed = start.elapsed();

    println!("Parsed {} dependencies in {:?}", 10_000, elapsed);

    if let Ok(tab) = result {
        assert_eq!(tab.runtime_dependencies.as_ref().unwrap().len(), 10_000);
    }

    // Should handle but might cause memory pressure
    // TODO: Consider dependency count limits
}

#[test]
fn attack_scanner_with_symlinked_cellar() {
    // ATTACK: Replace Cellar directory with symlink
    let tmp = TempDir::new().unwrap();

    // Create target directory
    let target = tmp.path().join("target");
    fs::create_dir_all(&target).unwrap();

    // Create fake Cellar as symlink
    let cellar_link = tmp.path().join("Cellar");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, &cellar_link).unwrap();

        let scanner = HomebrewScanner::with_prefix(tmp.path().to_path_buf());

        // Scanner follows symlinks currently
        // This could be used to scan arbitrary directories
        let result = scanner.scan();

        println!("Symlinked Cellar scan result: {:?}", result.is_ok());

        // TODO: Consider detecting and rejecting symlinked Cellar
    }
}

#[test]
fn attack_tab_with_special_characters() {
    // ATTACK: Tab with control characters and special Unicode
    let malicious_tab = r#"{
            "homebrew_version": "4.0.0",
            "installed_on_request": true,
            "runtime_dependencies": [
                {"full_name": "formula\n\r\t\0", "version": "1.0.0"}
            ]
        }"#;

    // Should either parse or reject gracefully
    let result = HomebrewTab::parse(malicious_tab);
    println!("Tab with special chars: {:?}", result.is_ok());
}

// ============================================================================
// FAKE HOMEBREW INSTALLATION
// ============================================================================

#[test]
fn attack_fake_cellar_structure() {
    // ATTACK: Create fake Homebrew with malicious kegs
    let tmp = TempDir::new().unwrap();
    let cellar = tmp.path().join("Cellar");

    // Create deeply nested structure to cause path length issues
    let mut deep_path = cellar.clone();
    for i in 0..100 {
        deep_path = deep_path.join(format!("level{}", i));
    }

    fs::create_dir_all(&deep_path).unwrap();

    let scanner = HomebrewScanner::with_prefix(tmp.path().to_path_buf());
    let result = scanner.scan();

    // Should handle deep nesting gracefully
    println!("Deep nesting scan: {:?}", result);
}
