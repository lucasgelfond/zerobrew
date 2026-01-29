use std::process::Command;

/// Represents a Homebrew package that can be migrated
#[derive(Debug, Clone)]
pub struct HomebrewPackage {
    pub name: String,
    pub tap: String,
    pub is_cask: bool,
}

/// Result of collecting Homebrew packages for migration
pub struct HomebrewMigrationPackages {
    /// Formulas from homebrew/core that can be migrated
    pub formulas: Vec<HomebrewPackage>,
    /// Formulas from non-core taps that cannot be migrated
    pub non_core_formulas: Vec<HomebrewPackage>,
    /// Cask packages that cannot be migrated
    pub casks: Vec<HomebrewPackage>,
}

/// Parse Homebrew formulas from JSON output of `brew info --json=v1 --installed`
pub fn parse_formulas_from_json(json: &serde_json::Value) -> Vec<HomebrewPackage> {
    let mut packages = Vec::new();

    if let Some(formulas) = json.as_array() {
        for formula in formulas {
            if let Some(name) = formula.get("name").and_then(|n| n.as_str()) {
                let tap = formula
                    .get("tap")
                    .and_then(|t| t.as_str())
                    .unwrap_or("homebrew/core")
                    .to_string();

                packages.push(HomebrewPackage {
                    name: name.to_string(),
                    tap,
                    is_cask: false,
                });
            }
        }
    }

    packages
}

/// Parse Homebrew casks from plain text output of `brew list --cask`
pub fn parse_casks_from_plain_text(output: &str) -> Vec<HomebrewPackage> {
    output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|name| HomebrewPackage {
            name: name.to_string(),
            tap: "homebrew/cask".to_string(),
            is_cask: true,
        })
        .collect()
}

/// Categorize Homebrew packages for migration
///
/// Returns a struct with separate lists for:
/// - Formulas from homebrew/core (migratable)
/// - Formulas from other taps (not migratable)
/// - Cask packages (not migratable)
pub fn categorize_packages(packages: &[HomebrewPackage]) -> HomebrewMigrationPackages {
    let mut formulas = Vec::new();
    let mut non_core_formulas = Vec::new();
    let mut casks = Vec::new();

    for pkg in packages {
        if pkg.is_cask {
            casks.push(pkg.clone());
        } else if pkg.tap == "homebrew/core" {
            formulas.push(pkg.clone());
        } else {
            non_core_formulas.push(pkg.clone());
        }
    }

    HomebrewMigrationPackages {
        formulas,
        non_core_formulas,
        casks,
    }
}

/// Get all installed Homebrew packages, categorized for migration
///
/// Only formulas from `homebrew/core` can be migrated to zerobrew.
/// Formulas from other taps and all casks are collected separately.
pub fn get_homebrew_packages() -> Result<HomebrewMigrationPackages, String> {
    // Get installed formulas
    let formulas_output = Command::new("brew")
        .args(["info", "--json=v1", "--installed"])
        .output()
        .map_err(|e| format!("Failed to run 'brew info': {}", e))?;

    if !formulas_output.status.success() {
        return Err(format!(
            "brew info failed: {}",
            String::from_utf8_lossy(&formulas_output.stderr)
        ));
    }

    let formulas_json: serde_json::Value = serde_json::from_slice(&formulas_output.stdout)
        .map_err(|e| format!("Failed to parse brew info JSON: {}", e))?;

    let formulas = parse_formulas_from_json(&formulas_json);

    // Get installed casks (plain text output, no JSON support)
    let casks_output = Command::new("brew")
        .args(["list", "--cask"])
        .output()
        .map_err(|e| format!("Failed to run 'brew list --cask': {}", e))?;

    if !casks_output.status.success() {
        return Err(format!(
            "brew list --cask failed: {}",
            String::from_utf8_lossy(&casks_output.stderr)
        ));
    }

    let casks = parse_casks_from_plain_text(&String::from_utf8_lossy(&casks_output.stdout));

    // Combine and categorize all packages
    let all_packages: Vec<HomebrewPackage> = formulas.into_iter().chain(casks).collect();
    Ok(categorize_packages(&all_packages))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_formulas_from_json() {
        let brew_output = r#"[
            {
                "name": "git",
                "tap": "homebrew/core",
                "versions": { "stable": "2.40.0" }
            },
            {
                "name": "neovim",
                "tap": "homebrew/core",
                "versions": { "stable": "0.9.0" }
            }
        ]"#;

        let formulas_json: serde_json::Value = serde_json::from_str(brew_output).unwrap();
        let packages = parse_formulas_from_json(&formulas_json);

        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "git");
        assert_eq!(packages[0].tap, "homebrew/core");
        assert!(!packages[0].is_cask);
        assert_eq!(packages[1].name, "neovim");
        assert!(!packages[1].is_cask);
    }

    #[test]
    fn test_parse_formulas_handles_missing_tap() {
        let brew_output = r#"[
            {"name": "no-tap-formula"}
        ]"#;

        let formulas_json: serde_json::Value = serde_json::from_str(brew_output).unwrap();
        let packages = parse_formulas_from_json(&formulas_json);

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "no-tap-formula");
        assert_eq!(packages[0].tap, "homebrew/core");
    }

    #[test]
    fn test_parse_casks_from_plain_text() {
        // Simulate brew list --cask output
        let brew_output = "visual-studio-code\nfirefox\n";

        let packages = parse_casks_from_plain_text(brew_output);

        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "visual-studio-code");
        assert_eq!(packages[0].tap, "homebrew/cask");
        assert!(packages[0].is_cask);
        assert_eq!(packages[1].name, "firefox");
        assert!(packages[1].is_cask);
    }

    #[test]
    fn test_parse_casks_handles_empty_output() {
        let brew_output = "";

        let packages = parse_casks_from_plain_text(brew_output);

        assert!(packages.is_empty());
    }

    #[test]
    fn test_parse_casks_handles_multiple_lines() {
        let brew_output = "visual-studio-code\nfirefox\ndocker\niterm2\n";

        let packages = parse_casks_from_plain_text(brew_output);

        assert_eq!(packages.len(), 4);
        assert_eq!(
            packages.iter().map(|p| &p.name).collect::<Vec<_>>(),
            vec!["visual-studio-code", "firefox", "docker", "iterm2"]
        );
    }

    #[test]
    fn test_categorize_packages_filters_core_formulas() {
        let packages = vec![
            HomebrewPackage {
                name: "git".to_string(),
                tap: "homebrew/core".to_string(),
                is_cask: false,
            },
            HomebrewPackage {
                name: "curl".to_string(),
                tap: "homebrew/core".to_string(),
                is_cask: false,
            },
        ];

        let result = categorize_packages(&packages);

        assert_eq!(result.formulas.len(), 2);
        assert!(result.non_core_formulas.is_empty());
        assert!(result.casks.is_empty());
    }

    #[test]
    fn test_categorize_packages_filters_non_core_formulas() {
        let packages = vec![
            HomebrewPackage {
                name: "php".to_string(),
                tap: "shivammathur/php".to_string(),
                is_cask: false,
            },
            HomebrewPackage {
                name: "mysql".to_string(),
                tap: "homebrew/mysql".to_string(),
                is_cask: false,
            },
        ];

        let result = categorize_packages(&packages);

        assert!(result.formulas.is_empty());
        assert_eq!(result.non_core_formulas.len(), 2);
        assert!(result.casks.is_empty());
    }

    #[test]
    fn test_categorize_packages_filters_casks() {
        let packages = vec![
            HomebrewPackage {
                name: "visual-studio-code".to_string(),
                tap: "homebrew/cask".to_string(),
                is_cask: true,
            },
            HomebrewPackage {
                name: "firefox".to_string(),
                tap: "homebrew/cask".to_string(),
                is_cask: true,
            },
        ];

        let result = categorize_packages(&packages);

        assert!(result.formulas.is_empty());
        assert!(result.non_core_formulas.is_empty());
        assert_eq!(result.casks.len(), 2);
    }

    #[test]
    fn test_categorize_packages_mixed_packages() {
        let packages = vec![
            HomebrewPackage {
                name: "git".to_string(),
                tap: "homebrew/core".to_string(),
                is_cask: false,
            },
            HomebrewPackage {
                name: "php".to_string(),
                tap: "homebrew/php".to_string(),
                is_cask: false,
            },
            HomebrewPackage {
                name: "visual-studio-code".to_string(),
                tap: "homebrew/cask".to_string(),
                is_cask: true,
            },
        ];

        let result = categorize_packages(&packages);

        assert_eq!(result.formulas.len(), 1);
        assert_eq!(result.formulas[0].name, "git");

        assert_eq!(result.non_core_formulas.len(), 1);
        assert_eq!(result.non_core_formulas[0].name, "php");

        assert_eq!(result.casks.len(), 1);
        assert_eq!(result.casks[0].name, "visual-studio-code");
    }

    #[test]
    fn test_homebrew_package_struct() {
        let pkg = HomebrewPackage {
            name: "test-formula".to_string(),
            tap: "homebrew/core".to_string(),
            is_cask: false,
        };

        assert_eq!(pkg.name, "test-formula");
        assert_eq!(pkg.tap, "homebrew/core");
        assert!(!pkg.is_cask);

        let cask = HomebrewPackage {
            name: "test-cask".to_string(),
            tap: "homebrew/cask".to_string(),
            is_cask: true,
        };

        assert!(cask.is_cask);
    }
}
