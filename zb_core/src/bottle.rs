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
            uses_from_macos: Vec::new(),
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
            uses_from_macos: Vec::new(),
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
            uses_from_macos: Vec::new(),
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
            uses_from_macos: Vec::new(),
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

    // ========================================================================
    // Linux-specific bottle selection tests
    // ========================================================================

    /// Test that Linux bottles are preferred over macOS bottles on Linux
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_prefers_linux_bottles_over_macos() {
        let mut files = BTreeMap::new();
        // Add both macOS and Linux bottles
        files.insert(
            "arm64_sonoma".to_string(),
            BottleFile {
                url: "https://example.com/macos.tar.gz".to_string(),
                sha256: "macos".to_string(),
            },
        );
        files.insert(
            "arm64_linux".to_string(),
            BottleFile {
                url: "https://example.com/linux-arm64.tar.gz".to_string(),
                sha256: "linux-arm64".to_string(),
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
            name: "test-pkg".to_string(),
            versions: Versions {
                stable: "1.0.0".to_string(),
            },
            dependencies: Vec::new(),
            uses_from_macos: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
        };

        let selected = select_bottle(&formula).unwrap();
        // Should select Linux bottle, not macOS
        assert!(
            selected.tag.contains("linux"),
            "Expected Linux bottle, got: {}",
            selected.tag
        );
    }

    /// Test fallback to 'all' bottle when no Linux-specific bottle exists
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_falls_back_to_all_bottle() {
        let mut files = BTreeMap::new();
        // Only macOS and 'all' bottles available
        files.insert(
            "arm64_sonoma".to_string(),
            BottleFile {
                url: "https://example.com/macos.tar.gz".to_string(),
                sha256: "macos".to_string(),
            },
        );
        files.insert(
            "all".to_string(),
            BottleFile {
                url: "https://example.com/all.tar.gz".to_string(),
                sha256: "all".to_string(),
            },
        );

        let formula = Formula {
            name: "ca-certs".to_string(),
            versions: Versions {
                stable: "1.0.0".to_string(),
            },
            dependencies: Vec::new(),
            uses_from_macos: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
        };

        let selected = select_bottle(&formula).unwrap();
        assert_eq!(selected.tag, "all");
    }

    /// Test that Linux x86_64 doesn't accidentally select arm64_linux
    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn x86_64_linux_does_not_select_arm64_linux() {
        let mut files = BTreeMap::new();
        // Only arm64_linux available (wrong arch)
        files.insert(
            "arm64_linux".to_string(),
            BottleFile {
                url: "https://example.com/arm64-linux.tar.gz".to_string(),
                sha256: "arm64".to_string(),
            },
        );

        let formula = Formula {
            name: "wrong-arch".to_string(),
            versions: Versions {
                stable: "1.0.0".to_string(),
            },
            dependencies: Vec::new(),
            uses_from_macos: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
        };

        // Should fail - no compatible bottle
        let err = select_bottle(&formula).unwrap_err();
        assert!(matches!(err, Error::UnsupportedBottle { .. }));
    }

    /// Test that arm64 Linux doesn't accidentally select x86_64_linux
    #[test]
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    fn arm64_linux_does_not_select_x86_64_linux() {
        let mut files = BTreeMap::new();
        // Only x86_64_linux available (wrong arch)
        files.insert(
            "x86_64_linux".to_string(),
            BottleFile {
                url: "https://example.com/x86-linux.tar.gz".to_string(),
                sha256: "x86".to_string(),
            },
        );

        let formula = Formula {
            name: "wrong-arch".to_string(),
            versions: Versions {
                stable: "1.0.0".to_string(),
            },
            dependencies: Vec::new(),
            uses_from_macos: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
        };

        // Should fail - no compatible bottle
        let err = select_bottle(&formula).unwrap_err();
        assert!(matches!(err, Error::UnsupportedBottle { .. }));
    }

    // ========================================================================
    // Cross-platform tests (run on both Linux and macOS)
    // ========================================================================

    /// Test that empty bottle list returns error
    #[test]
    fn empty_bottles_returns_error() {
        let files = BTreeMap::new();

        let formula = Formula {
            name: "empty".to_string(),
            versions: Versions {
                stable: "1.0.0".to_string(),
            },
            dependencies: Vec::new(),
            uses_from_macos: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
        };

        let err = select_bottle(&formula).unwrap_err();
        assert!(matches!(
            err,
            Error::UnsupportedBottle { name } if name == "empty"
        ));
    }

    /// Test that platform tags function returns non-empty on supported platforms
    #[test]
    fn platform_tags_non_empty_on_supported_platforms() {
        let tags = get_platform_tags();
        #[cfg(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64"),
        ))]
        assert!(!tags.is_empty(), "Expected non-empty platform tags");
    }

    /// Test Linux bottle selection using fixture
    #[test]
    #[cfg(target_os = "linux")]
    fn selects_linux_bottle_from_fixture() {
        let fixture = include_str!("../fixtures/formula_linux.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();

        let selected = select_bottle(&formula).unwrap();

        #[cfg(target_arch = "aarch64")]
        {
            assert_eq!(selected.tag, "arm64_linux");
            assert!(selected.url.contains("linux-arm"));
        }

        #[cfg(target_arch = "x86_64")]
        {
            assert_eq!(selected.tag, "x86_64_linux");
            assert!(selected.url.contains("linux-x86"));
        }
    }

    /// Test compatible fallback tag logic
    #[test]
    fn is_compatible_fallback_tag_logic() {
        #[cfg(target_os = "linux")]
        {
            // Linux should not consider macOS tags compatible
            assert!(!is_compatible_fallback_tag("arm64_sonoma"));
            assert!(!is_compatible_fallback_tag("arm64_ventura"));
            assert!(!is_compatible_fallback_tag("sonoma"));

            // Linux tags based on architecture
            #[cfg(target_arch = "aarch64")]
            {
                assert!(is_compatible_fallback_tag("arm64_linux"));
                assert!(!is_compatible_fallback_tag("x86_64_linux"));
            }
            #[cfg(target_arch = "x86_64")]
            {
                assert!(is_compatible_fallback_tag("x86_64_linux"));
                assert!(!is_compatible_fallback_tag("arm64_linux"));
            }
        }

        #[cfg(target_os = "macos")]
        {
            // macOS should not consider Linux tags compatible
            assert!(!is_compatible_fallback_tag("arm64_linux"));
            assert!(!is_compatible_fallback_tag("x86_64_linux"));
        }
    }

    // ========================================================================
    // Edge case tests for bottle selection
    // ========================================================================

    /// Test bottle selection with multiple versions of same platform
    #[test]
    fn prefers_newer_macos_version_tag() {
        // On macOS arm64, we prefer newer versions (tahoe > sequoia > sonoma)
        // This test verifies the ordering in get_platform_tags()
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            let tags = get_platform_tags();
            assert!(tags.len() >= 2, "Should have multiple macOS tags");
            // First tag should be newest
            assert!(tags[0].contains("tahoe") || tags[0].contains("sequoia"));
        }
    }

    /// Test that bottles with unusual but valid URLs work
    #[test]
    fn handles_ghcr_io_urls() {
        let mut files = BTreeMap::new();

        // Real Homebrew uses ghcr.io URLs
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let tag = "arm64_linux";
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let tag = "x86_64_linux";
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let tag = "arm64_sonoma";
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        let tag = "sonoma";
        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64"),
        )))]
        let tag = "all";

        files.insert(
            tag.to_string(),
            BottleFile {
                url: "https://ghcr.io/v2/homebrew/core/openssl%403.4/blobs/sha256:abc123def456".to_string(),
                sha256: "abc123def456".to_string(),
            },
        );

        let formula = Formula {
            name: "openssl@3.4".to_string(),
            versions: Versions {
                stable: "3.4.1".to_string(),
            },
            dependencies: Vec::new(),
            uses_from_macos: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
        };

        let selected = select_bottle(&formula).unwrap();
        assert!(selected.url.contains("ghcr.io"));
        assert!(selected.url.contains("openssl"));
    }

    /// Test bottle selection with URL-encoded characters
    #[test]
    fn handles_url_encoded_bottle_urls() {
        let mut files = BTreeMap::new();

        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let tag = "arm64_linux";
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let tag = "x86_64_linux";
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let tag = "arm64_sonoma";
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        let tag = "sonoma";
        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64"),
        )))]
        let tag = "all";

        files.insert(
            tag.to_string(),
            BottleFile {
                url: "https://example.com/bottles/pkg%2B%2B-1.0.0.tar.gz".to_string(),
                sha256: "encoded".to_string(),
            },
        );

        let formula = Formula {
            name: "pkg++".to_string(),
            versions: Versions {
                stable: "1.0.0".to_string(),
            },
            dependencies: Vec::new(),
            uses_from_macos: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
        };

        let selected = select_bottle(&formula).unwrap();
        assert!(selected.url.contains("%2B%2B"), "URL encoding should be preserved");
    }

    /// Test bottle with very long SHA256 (edge case validation)
    #[test]
    fn handles_standard_sha256_length() {
        let mut files = BTreeMap::new();

        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let tag = "arm64_linux";
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let tag = "x86_64_linux";
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let tag = "arm64_sonoma";
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        let tag = "sonoma";
        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64"),
        )))]
        let tag = "all";

        let valid_sha256 = "a".repeat(64); // Standard SHA256 is 64 hex chars

        files.insert(
            tag.to_string(),
            BottleFile {
                url: "https://example.com/test.tar.gz".to_string(),
                sha256: valid_sha256.clone(),
            },
        );

        let formula = Formula {
            name: "sha-test".to_string(),
            versions: Versions {
                stable: "1.0.0".to_string(),
            },
            dependencies: Vec::new(),
            uses_from_macos: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
        };

        let selected = select_bottle(&formula).unwrap();
        assert_eq!(selected.sha256.len(), 64);
    }

    /// Test that bottles with only incompatible architectures fail correctly
    #[test]
    fn rejects_incompatible_arch_only_bottles() {
        let mut files = BTreeMap::new();

        // Create bottle for a completely different architecture
        #[cfg(target_arch = "aarch64")]
        {
            // On arm64, only x86_64 bottles available
            files.insert(
                "sonoma".to_string(), // x86_64 macOS
                BottleFile {
                    url: "https://example.com/x86.tar.gz".to_string(),
                    sha256: "x86".to_string(),
                },
            );
        }
        #[cfg(target_arch = "x86_64")]
        {
            // On x86_64, only arm64 bottles available
            files.insert(
                "arm64_sonoma".to_string(),
                BottleFile {
                    url: "https://example.com/arm64.tar.gz".to_string(),
                    sha256: "arm64".to_string(),
                },
            );
        }

        let formula = Formula {
            name: "wrong-arch-pkg".to_string(),
            versions: Versions {
                stable: "1.0.0".to_string(),
            },
            dependencies: Vec::new(),
            uses_from_macos: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
        };

        let result = select_bottle(&formula);
        // Should fail because architecture doesn't match
        assert!(result.is_err(), "Should reject incompatible architecture");
    }

    /// Test bottle selection is deterministic (same input = same output)
    #[test]
    fn bottle_selection_is_deterministic() {
        let mut files = BTreeMap::new();

        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let tag = "arm64_linux";
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let tag = "x86_64_linux";
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let tag = "arm64_sonoma";
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        let tag = "sonoma";
        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64"),
        )))]
        let tag = "all";

        files.insert(
            tag.to_string(),
            BottleFile {
                url: "https://example.com/test.tar.gz".to_string(),
                sha256: "test123".to_string(),
            },
        );
        files.insert(
            "all".to_string(),
            BottleFile {
                url: "https://example.com/all.tar.gz".to_string(),
                sha256: "all123".to_string(),
            },
        );

        let formula = Formula {
            name: "deterministic".to_string(),
            versions: Versions {
                stable: "1.0.0".to_string(),
            },
            dependencies: Vec::new(),
            uses_from_macos: Vec::new(),
            bottle: Bottle {
                stable: BottleStable {
                    files: files.clone(),
                    rebuild: 0,
                },
            },
        };

        // Run selection multiple times
        let results: Vec<_> = (0..10).map(|_| select_bottle(&formula).unwrap()).collect();

        // All results should be identical
        for result in &results {
            assert_eq!(result.tag, results[0].tag);
            assert_eq!(result.url, results[0].url);
            assert_eq!(result.sha256, results[0].sha256);
        }
    }

    /// Test that formula name with special characters doesn't break selection
    #[test]
    fn handles_formula_names_with_special_chars() {
        let mut files = BTreeMap::new();

        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let tag = "arm64_linux";
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let tag = "x86_64_linux";
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let tag = "arm64_sonoma";
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        let tag = "sonoma";
        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64"),
        )))]
        let tag = "all";

        files.insert(
            tag.to_string(),
            BottleFile {
                url: "https://example.com/test.tar.gz".to_string(),
                sha256: "test".to_string(),
            },
        );

        // Test versioned formula name (e.g., python@3.12)
        let formula = Formula {
            name: "python@3.12".to_string(),
            versions: Versions {
                stable: "3.12.1".to_string(),
            },
            dependencies: Vec::new(),
            uses_from_macos: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
        };

        let selected = select_bottle(&formula).unwrap();
        assert!(!selected.tag.is_empty());
    }

    /// Test version strings with various formats
    #[test]
    fn handles_various_version_formats() {
        let test_versions = vec![
            "1.0.0",
            "1.0.0_1",       // With rebuild suffix
            "2024-01-01",    // Date-based
            "1.0.0-beta.1",  // Pre-release
            "0.0.1",         // Very low version
            "999.999.999",   // Very high version
        ];

        for version in test_versions {
            let mut files = BTreeMap::new();
            files.insert(
                "all".to_string(),
                BottleFile {
                    url: format!("https://example.com/pkg-{}.tar.gz", version),
                    sha256: "test".to_string(),
                },
            );

            let formula = Formula {
                name: "version-test".to_string(),
                versions: Versions {
                    stable: version.to_string(),
                },
                dependencies: Vec::new(),
                uses_from_macos: Vec::new(),
                bottle: Bottle {
                    stable: BottleStable { files, rebuild: 0 },
                },
            };

            let selected = select_bottle(&formula).unwrap();
            assert_eq!(selected.tag, "all", "Should select 'all' for version {}", version);
        }
    }
}
