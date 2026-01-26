use crate::{Error, Formula};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedBottle {
    pub tag: String,
    pub url: String,
    pub sha256: String,
}

/// Get the preferred bottle tags for the current platform
fn get_platform_tags() -> &'static [&'static str] {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        &[
            "arm64_tahoe",
            "arm64_sequoia",
            "arm64_sonoma",
            "arm64_ventura",
        ]
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        &["sonoma", "ventura", "monterey", "big_sur"]
    }

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        &["arm64_linux"]
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        &["x86_64_linux"]
    }

    // Fallback for other platforms (won't match anything)
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
    )))]
    {
        &[]
    }
}

/// Check if a tag is for the current platform family (for fallback selection)
fn is_compatible_fallback_tag(tag: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        // Any arm64 macOS bottle, but not linux
        tag.starts_with("arm64_") && !tag.contains("linux")
    }

    #[cfg(target_os = "linux")]
    {
        // Any linux bottle matching our architecture
        #[cfg(target_arch = "aarch64")]
        {
            tag == "arm64_linux"
        }
        #[cfg(target_arch = "x86_64")]
        {
            tag == "x86_64_linux"
        }
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        {
            false
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = tag;
        false
    }
}

pub fn select_bottle(formula: &Formula) -> Result<SelectedBottle, Error> {
    let platform_tags = get_platform_tags();

    // Try preferred tags for this platform (in order of preference)
    for preferred_tag in platform_tags {
        if let Some(file) = formula.bottle.stable.files.get(*preferred_tag) {
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

    // Fallback: any compatible bottle for this platform
    for (tag, file) in &formula.bottle.stable.files {
        if is_compatible_fallback_tag(tag) {
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

    // This test uses the fixture which only has macOS bottles
    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
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
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn selects_x86_64_linux_bottle() {
        let mut files = BTreeMap::new();
        files.insert(
            "x86_64_linux".to_string(),
            BottleFile {
                url: "https://ghcr.io/v2/homebrew/core/test/blobs/sha256:linux123".to_string(),
                sha256: "linux123".to_string(),
            },
        );
        files.insert(
            "arm64_sonoma".to_string(),
            BottleFile {
                url: "https://example.com/macos.tar.gz".to_string(),
                sha256: "macos123".to_string(),
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

        let selected = select_bottle(&formula).unwrap();
        assert_eq!(selected.tag, "x86_64_linux");
        assert!(selected.url.contains("linux123"));
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    fn selects_arm64_linux_bottle() {
        let mut files = BTreeMap::new();
        files.insert(
            "arm64_linux".to_string(),
            BottleFile {
                url: "https://ghcr.io/v2/homebrew/core/test/blobs/sha256:arm64linux".to_string(),
                sha256: "arm64linux".to_string(),
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

        let selected = select_bottle(&formula).unwrap();
        assert_eq!(selected.tag, "arm64_linux");
    }

    #[test]
    fn errors_when_no_compatible_bottle() {
        let mut files = BTreeMap::new();
        // Only has a bottle that won't match the current platform
        #[cfg(target_os = "macos")]
        files.insert(
            "x86_64_linux".to_string(),
            BottleFile {
                url: "https://example.com/linux.tar.gz".to_string(),
                sha256: "linux".to_string(),
            },
        );
        #[cfg(target_os = "linux")]
        files.insert(
            "arm64_sonoma".to_string(),
            BottleFile {
                url: "https://example.com/macos.tar.gz".to_string(),
                sha256: "macos".to_string(),
            },
        );

        let formula = Formula {
            name: "incompatible".to_string(),
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
            Error::UnsupportedBottle { name } if name == "incompatible"
        ));
    }
}
