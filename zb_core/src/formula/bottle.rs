use crate::{Error, Formula};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedBottle {
    pub tag: String,
    pub url: String,
    pub sha256: String,
}

const MACOS_CODENAMES_NEWEST_FIRST: &[&str] = &["tahoe", "sequoia", "sonoma", "ventura"];

#[cfg(target_os = "macos")]
pub fn macos_major_version() -> Option<u32> {
    let output = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()?;
    let version = String::from_utf8_lossy(&output.stdout);
    version.trim().split('.').next()?.parse().ok()
}

fn codename_for_major(major: u32) -> Option<&'static str> {
    match major {
        26 => Some("tahoe"),
        15 => Some("sequoia"),
        14 => Some("sonoma"),
        13 => Some("ventura"),
        _ => None,
    }
}

pub fn compatible_codenames(major_version: Option<u32>) -> Vec<&'static str> {
    let Some(major) = major_version else {
        return MACOS_CODENAMES_NEWEST_FIRST.to_vec();
    };

    let Some(pos) = codename_for_major(major)
        .and_then(|c| MACOS_CODENAMES_NEWEST_FIRST.iter().position(|&t| t == c))
    else {
        return MACOS_CODENAMES_NEWEST_FIRST.to_vec();
    };

    MACOS_CODENAMES_NEWEST_FIRST[pos..].to_vec()
}

pub fn select_bottle(formula: &Formula) -> Result<SelectedBottle, Error> {
    #[cfg(target_os = "macos")]
    let macos_version = macos_major_version();
    #[cfg(not(target_os = "macos"))]
    let macos_version: Option<u32> = None;

    select_bottle_with_version(formula, macos_version)
}

fn select_bottle_with_version(
    formula: &Formula,
    macos_version: Option<u32>,
) -> Result<SelectedBottle, Error> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let codenames = compatible_codenames(macos_version);
        let tags: Vec<String> = codenames.iter().map(|c| format!("arm64_{c}")).collect();

        for tag in &tags {
            if let Some(file) = formula.bottle.stable.files.get(tag.as_str()) {
                return Ok(SelectedBottle {
                    tag: tag.clone(),
                    url: file.url.clone(),
                    sha256: file.sha256.clone(),
                });
            }
        }
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        let tags = compatible_codenames(macos_version);

        for tag in &tags {
            if let Some(file) = formula.bottle.stable.files.get(*tag) {
                return Ok(SelectedBottle {
                    tag: tag.to_string(),
                    url: file.url.clone(),
                    sha256: file.sha256.clone(),
                });
            }
        }
    }

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

    if let Some(file) = formula.bottle.stable.files.get("all") {
        return Ok(SelectedBottle {
            tag: "all".to_string(),
            url: file.url.clone(),
            sha256: file.sha256.clone(),
        });
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let codenames = compatible_codenames(macos_version);
        for (tag, file) in &formula.bottle.stable.files {
            if tag.starts_with("arm64_") && !tag.contains("linux") {
                let bare = tag.strip_prefix("arm64_").unwrap_or(tag);
                if codenames.contains(&bare) {
                    return Ok(SelectedBottle {
                        tag: tag.clone(),
                        url: file.url.clone(),
                        sha256: file.sha256.clone(),
                    });
                }
            }
        }
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        let codenames = compatible_codenames(macos_version);
        for (tag, file) in &formula.bottle.stable.files {
            if !tag.starts_with("arm64_") && !tag.contains("linux") && tag != "all" {
                if codenames.contains(&tag.as_str()) {
                    return Ok(SelectedBottle {
                        tag: tag.clone(),
                        url: file.url.clone(),
                        sha256: file.sha256.clone(),
                    });
                }
            }
        }
    }

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
    use crate::formula::types::{Bottle, BottleFile, BottleStable, KegOnly, Versions};
    use std::collections::BTreeMap;

    #[test]
    fn selects_platform_bottle() {
        let fixture = include_str!("../../fixtures/formula_foo.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();

        let selected = select_bottle(&formula).unwrap();

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
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

        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        {
            assert_eq!(selected.tag, "sonoma");
            assert_eq!(
                selected.url,
                "https://example.com/foo-1.2.3.sonoma.bottle.tar.gz"
            );
            assert_eq!(
                selected.sha256,
                "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
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
            keg_only: KegOnly::default(),
            keg_only_reason: None,
            build_dependencies: Vec::new(),
            urls: None,
            ruby_source_path: None,
            ruby_source_checksum: None,
            uses_from_macos: Vec::new(),
            requirements: Vec::new(),
            variations: None,
        };

        let selected = select_bottle(&formula).unwrap();
        assert_eq!(selected.tag, "all");
        assert!(selected.url.contains("ca-certificates"));
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn errors_when_no_arm64_bottle() {
        let mut files = BTreeMap::new();
        files.insert(
            "sonoma".to_string(),
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
            keg_only: KegOnly::default(),
            keg_only_reason: None,
            build_dependencies: Vec::new(),
            urls: None,
            ruby_source_path: None,
            ruby_source_checksum: None,
            uses_from_macos: Vec::new(),
            requirements: Vec::new(),
            variations: None,
        };

        let err = select_bottle(&formula).unwrap_err();
        assert!(matches!(
            err,
            Error::UnsupportedBottle { name } if name == "legacy"
        ));
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    fn errors_when_no_x86_64_bottle() {
        let mut files = BTreeMap::new();
        files.insert(
            "arm64_sonoma".to_string(),
            BottleFile {
                url: "https://example.com/legacy.tar.gz".to_string(),
                sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
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
            keg_only: KegOnly::default(),
            keg_only_reason: None,
            build_dependencies: Vec::new(),
            urls: None,
            ruby_source_path: None,
            ruby_source_checksum: None,
            uses_from_macos: Vec::new(),
            requirements: Vec::new(),
            variations: None,
        };

        let err = select_bottle(&formula).unwrap_err();
        assert!(matches!(
            err,
            Error::UnsupportedBottle { name } if name == "legacy"
        ));
    }

    #[test]
    fn compatible_codenames_on_sequoia_excludes_tahoe() {
        let codenames = compatible_codenames(Some(15));
        assert_eq!(codenames, vec!["sequoia", "sonoma", "ventura"]);
    }

    #[test]
    fn compatible_codenames_on_tahoe_includes_all() {
        let codenames = compatible_codenames(Some(26));
        assert_eq!(codenames, vec!["tahoe", "sequoia", "sonoma", "ventura"]);
    }

    #[test]
    fn compatible_codenames_on_sonoma_excludes_newer() {
        let codenames = compatible_codenames(Some(14));
        assert_eq!(codenames, vec!["sonoma", "ventura"]);
    }

    #[test]
    fn compatible_codenames_on_ventura_returns_only_ventura() {
        let codenames = compatible_codenames(Some(13));
        assert_eq!(codenames, vec!["ventura"]);
    }

    #[test]
    fn compatible_codenames_unknown_version_returns_all() {
        let codenames = compatible_codenames(Some(99));
        assert_eq!(codenames, vec!["tahoe", "sequoia", "sonoma", "ventura"]);
    }

    #[test]
    fn compatible_codenames_none_returns_all() {
        let codenames = compatible_codenames(None);
        assert_eq!(codenames, vec!["tahoe", "sequoia", "sonoma", "ventura"]);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn sequoia_user_skips_tahoe_bottle() {
        let mut files = BTreeMap::new();
        files.insert(
            "arm64_tahoe".to_string(),
            BottleFile {
                url: "https://example.com/tahoe.tar.gz".to_string(),
                sha256: "aaaa".repeat(16),
            },
        );
        files.insert(
            "arm64_sequoia".to_string(),
            BottleFile {
                url: "https://example.com/sequoia.tar.gz".to_string(),
                sha256: "bbbb".repeat(16),
            },
        );

        let formula = Formula {
            name: "libpq".to_string(),
            versions: Versions {
                stable: "18.3".to_string(),
            },
            dependencies: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
            revision: 0,
            keg_only: KegOnly::default(),
            keg_only_reason: None,
            build_dependencies: Vec::new(),
            urls: None,
            ruby_source_path: None,
            ruby_source_checksum: None,
            uses_from_macos: Vec::new(),
            requirements: Vec::new(),
            variations: None,
        };

        let selected = select_bottle_with_version(&formula, Some(15)).unwrap();

        #[cfg(target_arch = "aarch64")]
        assert_eq!(selected.tag, "arm64_sequoia");

        #[cfg(target_arch = "x86_64")]
        assert_eq!(selected.tag, "all");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn tahoe_user_picks_tahoe_bottle() {
        let mut files = BTreeMap::new();
        files.insert(
            "arm64_tahoe".to_string(),
            BottleFile {
                url: "https://example.com/tahoe.tar.gz".to_string(),
                sha256: "aaaa".repeat(16),
            },
        );
        files.insert(
            "arm64_sequoia".to_string(),
            BottleFile {
                url: "https://example.com/sequoia.tar.gz".to_string(),
                sha256: "bbbb".repeat(16),
            },
        );

        let formula = Formula {
            name: "libpq".to_string(),
            versions: Versions {
                stable: "18.3".to_string(),
            },
            dependencies: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
            revision: 0,
            keg_only: KegOnly::default(),
            keg_only_reason: None,
            build_dependencies: Vec::new(),
            urls: None,
            ruby_source_path: None,
            ruby_source_checksum: None,
            uses_from_macos: Vec::new(),
            requirements: Vec::new(),
            variations: None,
        };

        let selected = select_bottle_with_version(&formula, Some(26)).unwrap();

        #[cfg(target_arch = "aarch64")]
        assert_eq!(selected.tag, "arm64_tahoe");

        #[cfg(target_arch = "x86_64")]
        assert_eq!(selected.tag, "all");
    }
}
