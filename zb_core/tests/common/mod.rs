use std::collections::BTreeMap;
use zb_core::formula::{Bottle, BottleFile, BottleStable, Formula, Versions};

/// Helper to create a minimal test formula with dependencies.
pub fn formula(name: &str, version: &str, deps: &[&str]) -> Formula {
    let mut files = BTreeMap::new();
    files.insert(
        "arm64_sonoma".to_string(),
        BottleFile {
            url: format!("https://example.com/{name}-{version}.tar.gz"),
            sha256: "deadbeef".repeat(8),
        },
    );

    Formula {
        name: name.to_string(),
        versions: Versions {
            stable: version.to_string(),
        },
        dependencies: deps.iter().map(|dep| dep.to_string()).collect(),
        bottle: Bottle {
            stable: BottleStable { files, rebuild: 0 },
        },
    }
}

// Helper to create a formula with a specific rebuild number.
/* pub fn formula_with_rebuild(name: &str, version: &str, rebuild: u32, deps: &[&str]) -> Formula {
    let mut files = BTreeMap::new();
    files.insert(
        "arm64_sonoma".to_string(),
        BottleFile {
            url: format!("https://example.com/{name}-{version}.tar.gz"),
            sha256: "deadbeef".repeat(8),
        },
    );

    Formula {
        name: name.to_string(),
        versions: Versions {
            stable: version.to_string(),
        },
        dependencies: deps.iter().map(|dep| dep.to_string()).collect(),
        bottle: Bottle {
            stable: BottleStable { files, rebuild },
        },
    }
} */
