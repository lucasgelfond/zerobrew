//! Security tests for core formula handling.

#[cfg(test)]
mod tests {
    use crate::formula::Formula;

    // ============================================================================
    // FORMULA NAME EXPLOITS
    // ============================================================================

    #[test]
    fn attack_formula_name_path_traversal() {
        // ATTACK: Formula with path traversal in name
        let malicious_json = r#"{
            "name": "../../../../../../etc/passwd",
            "versions": {"stable": "1.0.0"},
            "dependencies": [],
            "bottle": {
                "stable": {
                    "files": {
                        "arm64_sonoma": {
                            "url": "https://example.com/bottle.tar.gz",
                            "sha256": "abc123"
                        }
                    }
                }
            }
        }"#;

        let formula: Formula = serde_json::from_str(malicious_json).unwrap();

        // Name is parsed as-is
        assert_eq!(formula.name, "../../../../../../etc/passwd");

        // When this is used in paths like:
        // /opt/zerobrew/cellar/{formula.name}/{version}
        // It becomes: /opt/zerobrew/cellar/../../../../../../etc/passwd/1.0.0
        // Which resolves to: /etc/passwd/1.0.0

        // TODO: Validate formula.name is alphanumeric + limited chars
        // Reject: /, \, .., null bytes, control characters
    }

    #[test]
    fn attack_formula_version_path_traversal() {
        // ATTACK: Version with path traversal
        let malicious_json = r#"{
            "name": "innocent",
            "versions": {"stable": "../../../../../../../tmp/evil"},
            "dependencies": [],
            "bottle": {
                "stable": {
                    "files": {
                        "arm64_sonoma": {
                            "url": "https://example.com/bottle.tar.gz",
                            "sha256": "abc123"
                        }
                    }
                }
            }
        }"#;

        let formula: Formula = serde_json::from_str(malicious_json).unwrap();

        assert_eq!(formula.versions.stable, "../../../../../../../tmp/evil");

        // Path: /opt/zerobrew/cellar/innocent/../../../../../../../tmp/evil
        // Resolves to: /tmp/evil

        // TODO: Validate version strings don't contain path separators
    }

    #[test]
    fn attack_formula_name_with_null_bytes() {
        // ATTACK: Null bytes in formula name
        let name_with_null = "innocent\0malicious";
        let malicious_json = format!(
            r#"{{
                "name": "{}",
                "versions": {{"stable": "1.0.0"}},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "arm64_sonoma": {{
                                "url": "https://example.com/bottle.tar.gz",
                                "sha256": "abc123"
                            }}
                        }}
                    }}
                }}
            }}"#,
            name_with_null
        );

        // JSON parsing might stop at null byte
        let result: Result<Formula, _> = serde_json::from_str(&malicious_json);

        // Serde should either include the null or error
        if let Ok(formula) = result {
            // If it parsed, check if null byte is preserved
            println!("Formula name with null: {:?}", formula.name.as_bytes());
        }
    }

    #[test]
    fn attack_formula_name_extremely_long() {
        // ATTACK: DOS via extremely long formula name
        let long_name = "a".repeat(100_000);

        let malicious_json = format!(
            r#"{{
                "name": "{}",
                "versions": {{"stable": "1.0.0"}},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "arm64_sonoma": {{
                                "url": "https://example.com/bottle.tar.gz",
                                "sha256": "abc123"
                            }}
                        }}
                    }}
                }}
            }}"#,
            long_name
        );

        let start = std::time::Instant::now();
        let _result: Result<Formula, _> = serde_json::from_str(&malicious_json);
        let elapsed = start.elapsed();

        println!("Parsed {} char name in {:?}", long_name.len(), elapsed);

        // Should complete but highlights lack of length validation
        assert!(elapsed.as_secs() < 1);

        // TODO: Add formula name length limit (e.g., 256 chars)
    }

    // ============================================================================
    // DEPENDENCY EXPLOITS
    // ============================================================================

    #[test]
    fn attack_circular_dependencies() {
        // ATTACK: Formula with circular dependencies
        // Formula A depends on B, B depends on A

        // The resolver should detect this
        // This test documents expected behavior

        // TODO: Verify resolver handles circular deps gracefully
    }

    #[test]
    fn attack_dependency_name_injection() {
        // ATTACK: Dependency name with path traversal
        let malicious_json = r#"{
            "name": "innocent",
            "versions": {"stable": "1.0.0"},
            "dependencies": ["../../../../../../etc/passwd"],
            "bottle": {
                "stable": {
                    "files": {
                        "arm64_sonoma": {
                            "url": "https://example.com/bottle.tar.gz",
                            "sha256": "abc123"
                        }
                    }
                }
            }
        }"#;

        let formula: Formula = serde_json::from_str(malicious_json).unwrap();

        assert_eq!(formula.dependencies[0], "../../../../../../etc/passwd");

        // This dependency name will be used in API calls and path construction
        // Could cause path traversal if not validated

        // TODO: Validate dependency names same as formula names
    }

    // ============================================================================
    // BOTTLE URL EXPLOITS
    // ============================================================================

    #[test]
    fn attack_file_url_in_bottle() {
        // ATTACK: Try to use file:// URL to read local files
        let malicious_json = r#"{
            "name": "innocent",
            "versions": {"stable": "1.0.0"},
            "dependencies": [],
            "bottle": {
                "stable": {
                    "files": {
                        "arm64_sonoma": {
                            "url": "file:///etc/passwd",
                            "sha256": "abc123"
                        }
                    }
                }
            }
        }"#;

        let formula: Formula = serde_json::from_str(malicious_json).unwrap();

        assert_eq!(
            formula.bottle.stable.files.get("arm64_sonoma").unwrap().url,
            "file:///etc/passwd"
        );

        // reqwest should reject file:// URLs
        // But this test documents the attack vector
    }

    #[test]
    fn attack_javascript_url_in_bottle() {
        // ATTACK: Try to use javascript: or data: URL
        let schemes = vec!["javascript:alert(1)", "data:text/plain,malicious"];

        for scheme in schemes {
            let malicious_json = format!(
                r#"{{
                    "name": "innocent",
                    "versions": {{"stable": "1.0.0"}},
                    "dependencies": [],
                    "bottle": {{
                        "stable": {{
                            "files": {{
                                "arm64_sonoma": {{
                                    "url": "{}",
                                    "sha256": "abc123"
                                }}
                            }}
                        }}
                    }}
                }}"#,
                scheme
            );

            let formula: Result<Formula, _> = serde_json::from_str(&malicious_json);

            // Should parse, but reqwest will reject non-http(s) URLs
            if let Ok(f) = formula {
                println!(
                    "Parsed URL scheme: {}",
                    f.bottle.stable.files.get("arm64_sonoma").unwrap().url
                );
            }
        }
    }
}
