use console::style;
use std::path::PathBuf;

pub fn normalize_formula_name(name: &str) -> Result<String, zb_core::Error> {
    let trimmed = name.trim();
    if let Some(token) = trimmed.strip_prefix("cask:") {
        if token.is_empty() {
            return Err(zb_core::Error::InvalidArgument {
                message: "cask token cannot be empty".to_string(),
            });
        }
        return Ok(trimmed.to_string());
    }

    if let Some((tap, formula)) = trimmed.rsplit_once('/') {
        if formula.is_empty() {
            return Err(zb_core::Error::MissingFormula {
                name: trimmed.to_string(),
            });
        }

        if tap == "homebrew/core" {
            return Ok(formula.to_string());
        }

        if tap == "homebrew/cask" {
            return Ok(format!("cask:{formula}"));
        }

        return Ok(trimmed.to_string());
    }

    Ok(trimmed.to_string())
}

pub fn format_formula_suggestions(requested: &str, suggestions: &[String]) -> Option<String> {
    if suggestions.is_empty() {
        return None;
    }

    let mut rendered = format!(
        "{} Formula '{}' was not found. Did you mean:\n",
        style("Hint:").cyan().bold(),
        style(requested).bold()
    );

    for suggestion in suggestions {
        rendered.push_str(&format!("      {}\n", style(suggestion).green()));
    }

    Some(rendered)
}

pub fn suggest_formula_matches(requested: &str, suggestions: &[String]) {
    if let Some(message) = format_formula_suggestions(requested, suggestions) {
        eprintln!();
        eprint!("{message}");
        eprintln!();
    }
}

pub fn suggest_homebrew(formula: &str, error: &zb_core::Error) {
    eprintln!();
    eprintln!(
        "{} This package can't be installed with zerobrew.",
        style("Note:").yellow().bold()
    );
    eprintln!("      Error: {}", error);
    eprintln!();

    // Error for Termux on android since homebrew
    // doesn't support bottles for this platform
    // details: https://github.com/lucasgelfond/zerobrew/pull/136
    if cfg!(target_os = "android") {
        eprintln!(
            "      {} {}",
            style(formula).yellow().bold(),
            style(
                "is not compatible with Termux - homebrew bottles are not available for Android."
            )
            .red()
            .bold()
        );
        eprintln!(
            "      {}",
            style("and cannot be installed on it.").red().bold()
        );
    } else {
        eprintln!("      Try installing with Homebrew instead:");
        eprintln!(
            "      {}",
            style(format!("brew install {}", formula)).cyan()
        );
    }

    eprintln!();
}

pub fn get_root_path(cli_root: Option<PathBuf>) -> PathBuf {
    if let Some(root) = cli_root {
        return root;
    }

    if let Ok(env_root) = std::env::var("ZEROBREW_ROOT") {
        return PathBuf::from(env_root);
    }

    let legacy_root = PathBuf::from("/opt/zerobrew");
    if legacy_root.exists() {
        return legacy_root;
    }

    if cfg!(target_os = "macos") {
        legacy_root
    } else {
        let xdg_data_home = std::env::var("XDG_DATA_HOME")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var("HOME")
                    .map(|h| PathBuf::from(h).join(".local").join("share"))
                    .unwrap_or_else(|_| legacy_root.clone())
            });
        xdg_data_home.join("zerobrew")
    }
}

#[cfg(test)]
mod tests {
    use super::{format_formula_suggestions, normalize_formula_name};

    #[test]
    fn normalize_core_tap_formula() {
        assert_eq!(
            normalize_formula_name("homebrew/core/wget").unwrap(),
            "wget".to_string()
        );
    }

    #[test]
    fn normalize_external_tap_formula_keeps_full_name() {
        assert_eq!(
            normalize_formula_name("hashicorp/tap/terraform").unwrap(),
            "hashicorp/tap/terraform".to_string()
        );
    }

    #[test]
    fn normalize_homebrew_cask_prefixes_token() {
        assert_eq!(
            normalize_formula_name("homebrew/cask/docker-desktop").unwrap(),
            "cask:docker-desktop".to_string()
        );
    }

    #[test]
    fn format_formula_suggestions_renders_list() {
        let rendered =
            format_formula_suggestions("pythn", &["python".to_string(), "pytest".to_string()])
                .unwrap();

        assert!(rendered.contains("Did you mean"));
        assert!(rendered.contains("python"));
        assert!(rendered.contains("pytest"));
    }

    #[test]
    fn format_formula_suggestions_returns_none_for_empty_input() {
        assert!(format_formula_suggestions("pythn", &[]).is_none());
    }
}
