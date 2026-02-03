use std::collections::{BTreeMap, HashSet};

use regex::Regex;

use crate::api::ApiClient;
use zb_core::formula::{BinaryDownload, Bottle, BottleFile, BottleStable, Versions};
use zb_core::{Error, Formula};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TapRef {
    pub owner: String,
    pub repo: String,
}

impl TapRef {
    pub fn label(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

pub async fn fetch_formula(
    api_client: &ApiClient,
    tap: &TapRef,
    name: &str,
) -> Result<Formula, Error> {
    let base_url = tap_base_url();
    let repo_candidates = tap_repo_candidates(&tap.repo);
    let path_candidates = [
        format!("Formula/{name}.json"),
        format!("Formula/{name}.rb"),
        format!("HomebrewFormula/{name}.json"),
        format!("HomebrewFormula/{name}.rb"),
    ];

    for repo in repo_candidates {
        for path in &path_candidates {
            let url = format!("{}/{}/{}/HEAD/{}", base_url, tap.owner, repo, path);

            let Some(body) = api_client.get_text_if_exists(&url).await? else {
                continue;
            };

            if path.ends_with(".json") {
                let formula =
                    serde_json::from_str::<Formula>(&body).map_err(|e| Error::NetworkFailure {
                        message: format!("failed to parse tap formula JSON: {e}"),
                    })?;
                return Ok(formula);
            }

            if path.ends_with(".rb") {
                return parse_ruby_formula(name, &body);
            }
        }
    }

    Err(Error::MissingFormula {
        name: name.to_string(),
    })
}

fn tap_base_url() -> String {
    std::env::var("ZB_TAP_BASE_URL")
        .unwrap_or_else(|_| "https://raw.githubusercontent.com".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn tap_repo_candidates(repo: &str) -> Vec<String> {
    let mut repos = vec![repo.to_string()];
    if !repo.starts_with("homebrew-") {
        repos.push(format!("homebrew-{repo}"));
    }
    repos
}

fn parse_ruby_formula(name: &str, content: &str) -> Result<Formula, Error> {
    let version = extract_version(name, content)?;
    let revision = extract_revision(content);
    let dependencies = extract_dependencies(content);
    let bottle = extract_bottle(name, &version, content)?;
    let binary = if bottle.is_none() {
        extract_binary_download(name, content)?
    } else {
        None
    };
    let bottle = match bottle {
        Some(bottle) => bottle,
        None => {
            if binary.is_some() {
                Bottle {
                    stable: BottleStable {
                        files: BTreeMap::new(),
                        rebuild: 0,
                    },
                }
            } else {
                return Err(Error::UnsupportedBottle {
                    name: name.to_string(),
                });
            }
        }
    };

    Ok(Formula {
        name: name.to_string(),
        versions: Versions { stable: version },
        dependencies,
        bottle,
        binary,
        revision,
    })
}

fn extract_version(name: &str, content: &str) -> Result<String, Error> {
    let version_re = Regex::new(r#"(?m)^\s*version\s+["']([^"']+)["']"#).unwrap();
    if let Some(caps) = version_re.captures(content) {
        return Ok(caps[1].to_string());
    }

    let url_re = Regex::new(r#"(?m)^\s*url\s+["']([^"']+)["']"#).unwrap();
    if let Some(caps) = url_re.captures(content)
        && let Some(version) = version_from_url(&caps[1], name)
    {
        return Ok(version);
    }

    Err(Error::StoreCorruption {
        message: format!("tap formula '{name}' is missing a version"),
    })
}

fn version_from_url(url: &str, name: &str) -> Option<String> {
    let filename = url.rsplit('/').next()?;
    let trimmed = filename.trim_end_matches(".tar.gz");
    let trimmed = trimmed.trim_end_matches(".zip");
    let trimmed = trimmed.trim_end_matches(".tgz");

    if let Some(rest) = trimmed.strip_prefix(&format!("{name}-")) {
        return Some(rest.to_string());
    }

    let suffix = trimmed
        .split('-')
        .next_back()
        .filter(|s| s.chars().any(|c| c.is_ascii_digit()))?;
    Some(suffix.to_string())
}

fn extract_revision(content: &str) -> u32 {
    let revision_re = Regex::new(r#"(?m)^\s*revision\s+(\d+)"#).unwrap();
    revision_re
        .captures(content)
        .and_then(|caps| caps[1].parse::<u32>().ok())
        .unwrap_or(0)
}

fn extract_dependencies(content: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let dep_re = Regex::new(r#"(?m)^\s*depends_on\s+["']([^"']+)["']"#).unwrap();

    for caps in dep_re.captures_iter(content) {
        deps.push(caps[1].to_string());
    }

    deps.sort();
    deps.dedup();
    deps
}

fn extract_bottle(name: &str, version: &str, content: &str) -> Result<Option<Bottle>, Error> {
    let bottle_block = find_block(content, "bottle do");

    let Some(bottle_block) = bottle_block else {
        return Ok(None);
    };

    let root_url_re = Regex::new(r#"(?m)^\s*root_url\s+["']([^"']+)["']"#).unwrap();
    let root_url = root_url_re
        .captures(&bottle_block)
        .map(|caps| caps[1].to_string())
        .ok_or_else(|| Error::UnsupportedBottle {
            name: name.to_string(),
        })?;

    let rebuild_re = Regex::new(r#"(?m)^\s*rebuild\s+(\d+)"#).unwrap();
    let rebuild = rebuild_re
        .captures(&bottle_block)
        .and_then(|caps| caps[1].parse::<u32>().ok())
        .unwrap_or(0);

    let sha_re = Regex::new(r#"([a-z0-9_]+):\s*["']([a-f0-9]{64})["']"#).unwrap();
    let mut files = BTreeMap::new();
    let mut seen = HashSet::new();

    for caps in sha_re.captures_iter(&bottle_block) {
        let key = caps[1].to_string();
        let sha = caps[2].to_string();
        if key == "cellar" || key == "rebuild" {
            continue;
        }
        if !seen.insert(key.clone()) {
            continue;
        }

        let version_suffix = if rebuild > 0 {
            format!("{version}_{rebuild}")
        } else {
            version.to_string()
        };
        let url = format!("{root_url}/{name}-{version_suffix}.{key}.bottle.tar.gz");

        files.insert(key, BottleFile { url, sha256: sha });
    }

    if files.is_empty() {
        return Ok(None);
    }

    Ok(Some(Bottle {
        stable: BottleStable { files, rebuild },
    }))
}

fn extract_binary_download(name: &str, content: &str) -> Result<Option<BinaryDownload>, Error> {
    let url_re = Regex::new(r#"(?m)^\s*url\s+["']([^"']+)["']"#).unwrap();
    let sha_re = Regex::new(r#"(?m)^\s*sha256\s+["']([a-f0-9]{64})["']"#).unwrap();
    let bin_re = Regex::new(r#"(?m)^\s*bin\.install\s+\[?["']([^"']+)["']"#).unwrap();

    let url = url_re.captures(content).map(|caps| caps[1].to_string());
    let sha256 = sha_re.captures(content).map(|caps| caps[1].to_string());

    let Some(url) = url else {
        return Ok(None);
    };
    let Some(sha256) = sha256 else {
        return Ok(None);
    };

    let bin = bin_re
        .captures(content)
        .map(|caps| caps[1].to_string())
        .or_else(|| Some(name.to_string()));

    Ok(Some(BinaryDownload { url, sha256, bin }))
}

fn find_block(content: &str, marker: &str) -> Option<String> {
    let start = content.find(marker)?;
    let after = &content[start + marker.len()..];
    let mut depth = 1;
    let mut end = None;
    let mut lines = Vec::new();

    for line in after.lines() {
        if line.contains(" do") {
            depth += 1;
        }
        if line.trim() == "end" {
            depth -= 1;
            if depth == 0 {
                end = Some(());
                break;
            }
        }
        lines.push(line);
    }

    end?;
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_binary_only_formula() {
        let content = r#"
        class Gpd < Formula
          desc "gpd test"
          homepage "https://example.com"
          url "https://example.com/downloads/gpd"
          version "0.1.0"
          sha256 "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

          def install
            bin.install "gpd"
          end
        end
        "#;

        let formula = parse_ruby_formula("gpd", content).unwrap();
        assert_eq!(formula.name, "gpd");
        assert_eq!(formula.versions.stable, "0.1.0");
        assert!(formula.bottle.stable.files.is_empty());
        let binary = formula.binary.expect("binary download");
        assert_eq!(binary.url, "https://example.com/downloads/gpd");
        assert_eq!(
            binary.sha256,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(binary.bin.as_deref(), Some("gpd"));
    }
}
