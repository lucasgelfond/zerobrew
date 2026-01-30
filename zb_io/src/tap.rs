use std::path::PathBuf;
use zb_core::{Error};

pub struct TapManager {
    root: PathBuf,
}

impl TapManager {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn taps_dir(&self) -> PathBuf {
        self.root.join("Library").join("Taps")
    }

    pub fn tap_path(&self, user: &str, repo: &str) -> PathBuf {
        let repo_name = if repo.starts_with("homebrew-") {
            repo.to_string()
        } else {
            format!("homebrew-{}", repo)
        };
        self.taps_dir().join(user).join(repo_name)
    }

    /// Ensure a tap is present, cloning if necessary.
    /// In this implementation, we just check if it exists or return error as we don't have git logic here yet.
    /// Actually, for the port we should probably include the git clone logic if it was there.
    pub fn ensure_tap(&self, user: &str, repo: &str) -> Result<(PathBuf, bool), Error> {
        let tap_dir = self.tap_path(user, repo);
        if tap_dir.exists() {
            return Ok((tap_dir, false));
        }

        let repo_name = if repo.starts_with("homebrew-") {
            repo.to_string()
        } else {
            format!("homebrew-{}", repo)
        };
        let url = format!("https://github.com/{}/{}", user, repo_name);

        std::fs::create_dir_all(tap_dir.parent().unwrap()).map_err(|e| Error::IoError(e.to_string()))?;

        // Simple git clone
        let status = std::process::Command::new("git")
            .arg("clone")
            .arg("--depth=1")
            .arg(&url)
            .arg(&tap_dir)
            .status()
            .map_err(|e| Error::IoError(e.to_string()))?;

        if !status.success() {
            return Err(Error::IoError(format!("Failed to clone tap: {}", url)));
        }

        Ok((tap_dir, true))
    }

    pub fn untap(&self, name: &str) -> Result<(), Error> {
        let (user, repo) = name.split_once('/').ok_or_else(|| Error::ParseError {
            message: "Invalid tap name, expected user/repo".to_string(),
        })?;

        let tap_dir = self.tap_path(user, repo);
        if !tap_dir.exists() {
            return Ok(());
        }

        std::fs::remove_dir_all(&tap_dir).map_err(|e| Error::IoError(format!("Failed to remove tap dir: {}", e)))?;
        Ok(())
    }

    /// List installed taps
    pub fn list_taps(&self) -> Vec<String> {
        let mut taps = Vec::new();
        if let Ok(users) = std::fs::read_dir(self.taps_dir()) {
            for user_entry in users.flatten() {
                if !user_entry.path().is_dir() {
                    continue;
                }
                let user = user_entry.file_name();
                let user_str = user.to_string_lossy();

                if let Ok(repos) = std::fs::read_dir(user_entry.path()) {
                    for repo_entry in repos.flatten() {
                        if !repo_entry.path().is_dir() {
                            continue;
                        }
                        let repo = repo_entry.file_name();
                        let repo_str = repo.to_string_lossy();

                        let short_repo = if repo_str.starts_with("homebrew-") {
                            repo_str.trim_start_matches("homebrew-")
                        } else {
                            &repo_str
                        };

                        taps.push(format!("{}/{}", user_str, short_repo));
                    }
                }
            }
        }
        taps.sort();
        taps
    }

    /// Resolve a formula by name from a tap
    /// name must be "user/repo/formula"
    pub fn resolve_formula(&self, name: &str) -> Result<zb_core::Formula, Error> {
        if let Some((user, rest)) = name.split_once('/') {
            if let Some((repo, formula_name)) = rest.split_once('/') {
                let (tap_dir, _) = self.ensure_tap(user, repo)?;

                // Try Formula subdirectory
                let formula_path = tap_dir.join("Formula").join(format!("{}.rb", formula_name));
                if formula_path.exists() {
                    return crate::formula_parser::FormulaParser::parse_file(&formula_path, formula_name);
                }

                // Try HomebrewFormula subdirectory
                let formula_path = tap_dir.join("HomebrewFormula").join(format!("{}.rb", formula_name));
                if formula_path.exists() {
                    return crate::formula_parser::FormulaParser::parse_file(&formula_path, formula_name);
                }

                // Try root directory
                let formula_path = tap_dir.join(format!("{}.rb", formula_name));
                if formula_path.exists() {
                    return crate::formula_parser::FormulaParser::parse_file(&formula_path, formula_name);
                }

                return Err(Error::MissingFormula {
                    name: name.to_string(),
                });
            }
        }
        
        Err(Error::MissingFormula {
            name: name.to_string(),
        })
    }

    /// Find a formula by its short name in any installed tap
    pub fn find_formula(&self, short_name: &str) -> Option<zb_core::Formula> {
        let taps = self.list_taps();
        for tap in taps {
            if let Some((user, repo)) = tap.split_once('/') {
                let full_name = format!("{}/{}/{}", user, repo, short_name);
                if let Ok(formula) = self.resolve_formula(&full_name) {
                    return Some(formula);
                }
            }
        }
        None
    }

    /// List all available items (formulas) from installed taps
    pub fn list_available_items(&self) -> Vec<String> {
        let mut items = Vec::new();

        if let Ok(users) = std::fs::read_dir(self.taps_dir()) {
            for user_entry in users.flatten() {
                if !user_entry.path().is_dir() { continue; }
                let user_str = user_entry.file_name().to_string_lossy().to_string();

                if let Ok(repos) = std::fs::read_dir(user_entry.path()) {
                    for repo_entry in repos.flatten() {
                        if !repo_entry.path().is_dir() {
                            continue;
                        }
                        let repo = repo_entry.file_name();
                        let repo_str = repo.to_string_lossy();
                        
                        let short_repo = if repo_str.starts_with("homebrew-") {
                            repo_str.trim_start_matches("homebrew-")
                        } else {
                            &repo_str
                        };

                        // Scan Formula and HomebrewFormula
                        for subdir in &["Formula", "HomebrewFormula"] {
                            let dir = repo_entry.path().join(subdir);
                            if dir.exists() {
                                if let Ok(files) = std::fs::read_dir(dir) {
                                    for file in files.flatten() {
                                        let path = file.path();
                                        if path.extension().is_some_and(|e| e == "rb") {
                                            if let Some(stem) = path.file_stem() {
                                                items.push(stem.to_string_lossy().to_string());
                                                items.push(format!("{}/{}/{}", user_str, short_repo, stem.to_string_lossy()));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        // Also check root
                        if let Ok(files) = std::fs::read_dir(repo_entry.path()) {
                             for file in files.flatten() {
                                let path = file.path();
                                if path.is_file() && path.extension().is_some_and(|e| e == "rb") {
                                    if let Some(stem) = path.file_stem() {
                                         items.push(stem.to_string_lossy().to_string());
                                         items.push(format!("{}/{}/{}", user_str, short_repo, stem.to_string_lossy()));
                                    }
                                }
                             }
                        }
                    }
                }
            }
        }

        items.sort();
        items.dedup();
        items
    }
}
