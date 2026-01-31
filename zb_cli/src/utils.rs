use console::style;
use std::path::PathBuf;

pub fn normalize_formula_name(name: &str) -> Result<String, zb_core::Error> {
    let trimmed = name.trim();
    if let Some((tap, formula)) = trimmed.rsplit_once('/') {
        if tap == "homebrew/core" {
            if formula.is_empty() {
                return Err(zb_core::Error::MissingFormula {
                    name: trimmed.to_string(),
                });
            }
            return Ok(formula.to_string());
        }
        return Err(zb_core::Error::UnsupportedTap {
            name: trimmed.to_string(),
        });
    }

    Ok(trimmed.to_string())
}

pub fn suggest_homebrew(formula: &str, error: &zb_core::Error) {
    eprintln!();
    eprintln!(
        "{} This package can't be installed with zerobrew.",
        style("Note:").yellow().bold()
    );
    eprintln!("      Error: {}", error);
    eprintln!();

    // Return Error for termux on android since homebrew
    // doesn't support bottles for this platform
    if cfg!(target_os = "android") {
        eprintln!(
            "      {} {}",
            style(formula).yellow().bold(),
            style("is not compatible with termux on android")
                .red()
                .bold()
        );
        eprintln!(
            "      {}",
            style("and can not be installed on it.").red().bold()
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
