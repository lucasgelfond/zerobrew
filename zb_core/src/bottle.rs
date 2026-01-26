use crate::{Error, Formula};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedBottle {
    pub tag: String,
    pub url: String,
    pub sha256: String,
}

/// Get the current macOS major version (e.g., 15 for Sequoia)
#[cfg(target_os = "macos")]
fn get_macos_major_version() -> Option<u32> {
    use std::process::Command;

    let output = Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()?;

    let version_str = String::from_utf8_lossy(&output.stdout);
    let major = version_str.trim().split('.').next()?;
    major.parse().ok()
}

#[cfg(not(target_os = "macos"))]
fn get_macos_major_version() -> Option<u32> {
    None
}

/// Map bottle tag to minimum required macOS version
fn bottle_tag_macos_version(tag: &str) -> Option<u32> {
    match tag {
        "arm64_tahoe" => Some(16),
        "arm64_sequoia" => Some(15),
        "arm64_sonoma" => Some(14),
        "arm64_ventura" => Some(13),
        _ => None,
    }
}

pub fn select_bottle(formula: &Formula) -> Result<SelectedBottle, Error> {
    // Get current macOS version (fallback to oldest supported if detection fails)
    let current_macos = get_macos_major_version().unwrap_or(13);
    select_bottle_for_macos_version(formula, current_macos)
}

fn select_bottle_for_macos_version(
    formula: &Formula,
    current_macos: u32,
) -> Result<SelectedBottle, Error> {
    // Prefer macOS ARM bottles in order of preference (newest first)
    // but only if compatible with current macOS version
    let macos_tags = [
        "arm64_tahoe",
        "arm64_sequoia",
        "arm64_sonoma",
        "arm64_ventura",
    ];

    for preferred_tag in macos_tags {
        // Skip bottles that require a newer macOS than we have
        if let Some(required_version) = bottle_tag_macos_version(preferred_tag)
            && required_version > current_macos
        {
            continue;
        }

        if let Some(file) = formula.bottle.stable.files.get(preferred_tag) {
            return Ok(SelectedBottle {
                tag: preferred_tag.to_string(),
                url: file.url.clone(),
                sha256: file.sha256.clone(),
            });
        }
    }

    // Check for universal "all" bottle (platform-independent packages like ca-certificates)
    if let Some(file) = formula.bottle.stable.files.get("all") {
        return Ok(SelectedBottle {
            tag: "all".to_string(),
            url: file.url.clone(),
            sha256: file.sha256.clone(),
        });
    }

    // Fallback: any arm64 macOS bottle (but not linux)
    for (tag, file) in &formula.bottle.stable.files {
        if tag.starts_with("arm64_") && !tag.contains("linux") {
            return Ok(SelectedBottle {
                tag: tag.clone(),
                url: file.url.clone(),
                sha256: file.sha256.clone(),
            });
        }
    }

    Err(Error::UnsupportedBottle {
        name: formula.name.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::{Bottle, BottleFile, BottleStable, Versions};
    use std::collections::BTreeMap;

    #[test]
    fn selects_arm64_bottle() {
        let fixture = include_str!("../fixtures/formula_foo.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();

        let selected = select_bottle(&formula).unwrap();
        assert_eq!(selected.tag, "arm64_sonoma");
        assert_eq!(
            selected.url,
            "https://example.com/foo-1.2.3.arm64_sonoma.bottle.tar.gz"
        );
        assert_eq!(
            selected.sha256,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn selects_all_bottle_for_universal_packages() {
        let mut files = BTreeMap::new();
        files.insert(
            "all".to_string(),
            BottleFile {
                url: "https://ghcr.io/v2/homebrew/core/ca-certificates/blobs/sha256:abc123"
                    .to_string(),
                sha256: "abc123".to_string(),
            },
        );

        let formula = Formula {
            name: "ca-certificates".to_string(),
            versions: Versions {
                stable: "2024-01-01".to_string(),
            },
            dependencies: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
        };

        let selected = select_bottle(&formula).unwrap();
        assert_eq!(selected.tag, "all");
        assert!(selected.url.contains("ca-certificates"));
    }

    #[test]
    fn errors_when_no_arm64_bottle() {
        let mut files = BTreeMap::new();
        files.insert(
            "x86_64_sonoma".to_string(),
            BottleFile {
                url: "https://example.com/legacy.tar.gz".to_string(),
                sha256: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                    .to_string(),
            },
        );

        let formula = Formula {
            name: "legacy".to_string(),
            versions: Versions {
                stable: "0.1.0".to_string(),
            },
            dependencies: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
        };

        let err = select_bottle(&formula).unwrap_err();
        assert!(matches!(
            err,
            Error::UnsupportedBottle { name } if name == "legacy"
        ));
    }

    #[test]
    fn skips_incompatible_bottles_on_older_macos() {
        let mut files = BTreeMap::new();
        files.insert(
            "arm64_tahoe".to_string(),
            BottleFile {
                url: "https://example.com/tahoe.tar.gz".to_string(),
                sha256: "tahoe_sha".to_string(),
            },
        );
        files.insert(
            "arm64_sequoia".to_string(),
            BottleFile {
                url: "https://example.com/sequoia.tar.gz".to_string(),
                sha256: "sequoia_sha".to_string(),
            },
        );
        files.insert(
            "arm64_sonoma".to_string(),
            BottleFile {
                url: "https://example.com/sonoma.tar.gz".to_string(),
                sha256: "sonoma_sha".to_string(),
            },
        );

        let formula = Formula {
            name: "test-pkg".to_string(),
            versions: Versions {
                stable: "1.0.0".to_string(),
            },
            dependencies: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
        };

        // On macOS 15 (Sequoia), should skip tahoe (requires 16) and select sequoia
        let selected = select_bottle_for_macos_version(&formula, 15).unwrap();
        assert_eq!(selected.tag, "arm64_sequoia");

        // On macOS 14 (Sonoma), should skip tahoe and sequoia, select sonoma
        let selected = select_bottle_for_macos_version(&formula, 14).unwrap();
        assert_eq!(selected.tag, "arm64_sonoma");

        // On macOS 16 (Tahoe), should select tahoe (newest compatible)
        let selected = select_bottle_for_macos_version(&formula, 16).unwrap();
        assert_eq!(selected.tag, "arm64_tahoe");
    }

    #[test]
    fn bottle_tag_version_mapping() {
        assert_eq!(bottle_tag_macos_version("arm64_tahoe"), Some(16));
        assert_eq!(bottle_tag_macos_version("arm64_sequoia"), Some(15));
        assert_eq!(bottle_tag_macos_version("arm64_sonoma"), Some(14));
        assert_eq!(bottle_tag_macos_version("arm64_ventura"), Some(13));
        assert_eq!(bottle_tag_macos_version("all"), None);
        assert_eq!(bottle_tag_macos_version("arm64_unknown"), None);
    }
}
