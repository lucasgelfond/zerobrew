use crate::{Error, Formula};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedBottle {
    pub tag: String,
    pub url: String,
    pub sha256: String,
}

pub fn select_bottle(formula: &Formula) -> Result<SelectedBottle, Error> {
    // Prefer macOS ARM bottles in order of preference (newest first)
    #[cfg(target_os = "macos")]
    {
        let macos_tags = [
            "arm64_tahoe",
            "arm64_sequoia",
            "arm64_sonoma",
            "arm64_ventura",
        ];

        for preferred_tag in macos_tags {
            if let Some(file) = formula.bottle.stable.files.get(preferred_tag) {
                return Ok(SelectedBottle {
                    tag: preferred_tag.to_string(),
                    url: file.url.clone(),
                    sha256: file.sha256.clone(),
                });
            }
        }
    }

    // Prefer Linux x86_64 bottles
    #[cfg(target_os = "linux")]
    {
        let linux_tags = ["x86_64_linux"];
        for preferred_tag in linux_tags {
            if let Some(file) = formula.bottle.stable.files.get(preferred_tag) {
                return Ok(SelectedBottle {
                    tag: preferred_tag.to_string(),
                    url: file.url.clone(),
                    sha256: file.sha256.clone(),
                });
            }
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
    #[cfg(target_os = "macos")]
    for (tag, file) in &formula.bottle.stable.files {
        if tag.starts_with("arm64_") && !tag.contains("linux") {
            return Ok(SelectedBottle {
                tag: tag.clone(),
                url: file.url.clone(),
                sha256: file.sha256.clone(),
            });
        }
    }

    // Fallback for Linux: any linux bottle
    #[cfg(target_os = "linux")]
    for (tag, file) in &formula.bottle.stable.files {
        if tag.contains("linux") {
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
    fn selects_platform_bottle() {
        let fixture = include_str!("../fixtures/formula_foo.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();

        let selected = select_bottle(&formula).unwrap();

        #[cfg(target_os = "macos")]
        {
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

        #[cfg(target_os = "linux")]
        {
            assert_eq!(selected.tag, "x86_64_linux");
            assert_eq!(
                selected.url,
                "https://example.com/foo-1.2.3.x86_64_linux.bottle.tar.gz"
            );
            assert_eq!(
                selected.sha256,
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            );
        }
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
            revision: 0,
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
            revision: 0,
        };

        let err = select_bottle(&formula).unwrap_err();
        assert!(matches!(
            err,
            Error::UnsupportedBottle { name } if name == "legacy"
        ));
    }
}
