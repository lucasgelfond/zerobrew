use regex::Regex;
use std::collections::BTreeMap;
use zb_core::formula::{Bottle, BottleFile, BottleStable, Versions};
use zb_core::{Error, Formula};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TapFormulaRef {
    pub owner: String,
    pub repo: String,
    pub formula: String,
}

pub fn parse_tap_formula_ref(input: &str) -> Option<TapFormulaRef> {
    let mut parts = input.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    let formula = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if owner.is_empty() || repo.is_empty() || formula.is_empty() {
        return None;
    }
    Some(TapFormulaRef {
        owner: owner.to_string(),
        repo: repo.to_string(),
        formula: formula.to_string(),
    })
}

pub fn parse_tap_formula_ruby(spec: &TapFormulaRef, source: &str) -> Result<Formula, Error> {
    let stable = parse_version(source).unwrap_or_else(|| "0".to_string());
    let revision = parse_revision(source).unwrap_or(0);
    let dependencies = parse_dependencies(source);
    let bottle = parse_bottle(spec, source, &stable, revision)?;

    Ok(Formula {
        name: spec.formula.clone(),
        versions: Versions { stable },
        dependencies,
        bottle,
        revision,
    })
}

fn parse_version(source: &str) -> Option<String> {
    let explicit_re = Regex::new(r#"(?m)^\s*version\s+["']([^"']+)["']"#).ok()?;
    if let Some(v) = explicit_re
        .captures(source)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
    {
        return Some(v);
    }

    let url_version_re = Regex::new(
        r#"(?m)^\s*url\s+["'][^"']*(?:refs/tags|archive|download)/v?([0-9][0-9A-Za-z._+-]*)"#,
    )
    .ok()?;
    url_version_re
        .captures(source)
        .and_then(|c| c.get(1))
        .map(|m| normalize_inferred_version(m.as_str()))
}

fn normalize_inferred_version(raw: &str) -> String {
    let mut v = raw.to_string();
    for suffix in [".tar.gz", ".tar.xz", ".tar.bz2", ".tgz", ".zip"] {
        if v.ends_with(suffix) {
            v.truncate(v.len() - suffix.len());
            break;
        }
    }
    v
}

fn parse_revision(source: &str) -> Option<u32> {
    let revision_re = Regex::new(r#"(?m)^\s*revision\s+(\d+)\s*$"#).ok()?;
    revision_re
        .captures(source)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u32>().ok())
}

fn parse_dependencies(source: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let dep_re = Regex::new(r#"(?m)^\s*depends_on\s+["']([^"']+)["'](.*)$"#)
        .expect("depends_on regex must compile");
    for cap in dep_re.captures_iter(source) {
        let options = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        if options.contains(":build") || options.contains(":test") {
            continue;
        }
        if let Some(dep) = cap.get(1) {
            deps.push(dep.as_str().to_string());
        }
    }
    deps.sort();
    deps.dedup();
    deps
}

fn parse_bottle(
    spec: &TapFormulaRef,
    source: &str,
    stable: &str,
    revision: u32,
) -> Result<Bottle, Error> {
    let block = extract_bottle_block(source).ok_or_else(|| Error::MissingFormula {
        name: format!(
            "tap formula '{}' does not contain a bottle block",
            spec.formula
        ),
    })?;

    let root_url = parse_root_url(block)
        .unwrap_or_else(|| format!("https://ghcr.io/v2/{}/{}", spec.owner, spec.repo));
    let rebuild = parse_rebuild(block).unwrap_or(0);
    let files = parse_bottle_files(spec, &root_url, stable, revision, rebuild, block);

    if files.is_empty() {
        return Err(Error::MissingFormula {
            name: format!(
                "tap formula '{}' does not contain supported bottle sha256 entries",
                spec.formula
            ),
        });
    }

    Ok(Bottle {
        stable: BottleStable { files, rebuild },
    })
}

fn extract_bottle_block(source: &str) -> Option<&str> {
    let bottle_start_re = Regex::new(r#"^\s*bottle\s+do\b"#).ok()?;
    let end_re = Regex::new(r#"^\s*end\b"#).ok()?;
    let do_re = Regex::new(r#"\bdo\b"#).ok()?;
    let keyword_start_re =
        Regex::new(r#"^\s*(if|unless|case|begin|def|class|module|for|while|until)\b"#).ok()?;

    let mut offset = 0usize;
    let mut bottle_body_start: Option<usize> = None;
    let mut depth = 0usize;

    for line in source.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let trimmed = line.trim();

        if bottle_body_start.is_none() {
            if bottle_start_re.is_match(trimmed) {
                bottle_body_start = Some(offset);
                depth = 1;
            }
            continue;
        }

        if end_re.is_match(trimmed) {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return bottle_body_start.map(|start| &source[start..line_start]);
            }
            continue;
        }

        depth += do_re.find_iter(trimmed).count();
        if keyword_start_re.is_match(trimmed) {
            depth += 1;
        }
    }

    None
}

fn parse_root_url(block: &str) -> Option<String> {
    let root_re = Regex::new(r#"root_url\s+["']([^"']+)["']"#).ok()?;
    root_re
        .captures(block)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn parse_rebuild(block: &str) -> Option<u32> {
    let rebuild_re = Regex::new(r#"(?m)^\s*rebuild\s+(\d+)\s*$"#).ok()?;
    rebuild_re
        .captures(block)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u32>().ok())
}

fn parse_bottle_files(
    spec: &TapFormulaRef,
    root_url: &str,
    stable: &str,
    revision: u32,
    rebuild: u32,
    block: &str,
) -> BTreeMap<String, BottleFile> {
    let mut files = BTreeMap::new();
    let sha_re = Regex::new(r#"([a-z0-9_]+):\s*"([0-9a-f]{64})""#).expect("sha regex must compile");

    for cap in sha_re.captures_iter(block) {
        let Some(tag) = cap.get(1).map(|m| m.as_str()) else {
            continue;
        };
        let Some(sha) = cap.get(2).map(|m| m.as_str()) else {
            continue;
        };
        if matches!(tag, "cellar") {
            continue;
        }
        let url = build_bottle_url(spec, root_url, stable, revision, rebuild, tag, sha);
        files.insert(
            tag.to_string(),
            BottleFile {
                url,
                sha256: sha.to_string(),
            },
        );
    }

    files
}

fn build_bottle_url(
    spec: &TapFormulaRef,
    root_url: &str,
    stable: &str,
    revision: u32,
    rebuild: u32,
    tag: &str,
    sha: &str,
) -> String {
    let normalized = root_url.trim_end_matches('/');
    if normalized.contains("/v2/") {
        return format!("{}/{}/blobs/sha256:{}", normalized, spec.formula, sha);
    }

    let effective_version = if revision > 0 {
        format!("{stable}_{revision}")
    } else {
        stable.to_string()
    };

    if rebuild > 0 {
        format!(
            "{}/{}-{}.{}.{}.bottle.tar.gz",
            normalized, spec.formula, effective_version, rebuild, tag
        )
    } else {
        format!(
            "{}/{}-{}.{}.bottle.tar.gz",
            normalized, spec.formula, effective_version, tag
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tap_formula_reference() {
        let parsed = parse_tap_formula_ref("hashicorp/tap/terraform").unwrap();
        assert_eq!(parsed.owner, "hashicorp");
        assert_eq!(parsed.repo, "tap");
        assert_eq!(parsed.formula, "terraform");
    }

    #[test]
    fn rejects_non_tap_reference() {
        assert!(parse_tap_formula_ref("jq").is_none());
        assert!(parse_tap_formula_ref("a/b").is_none());
        assert!(parse_tap_formula_ref("a/b/c/d").is_none());
    }

    #[test]
    fn parses_formula_subset_with_bottle_data() {
        let source = r#"
class Terraform < Formula
  version "1.10.0"
  revision 1
  depends_on "go" => :build
  depends_on "openssl@3"

  bottle do
    root_url "https://ghcr.io/v2/hashicorp/tap"
    rebuild 2
    sha256 cellar: :any_skip_relocation, arm64_sonoma: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    sha256 cellar: :any_skip_relocation, x86_64_linux: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
  end
end
"#;

        let spec = TapFormulaRef {
            owner: "hashicorp".to_string(),
            repo: "tap".to_string(),
            formula: "terraform".to_string(),
        };

        let formula = parse_tap_formula_ruby(&spec, source).unwrap();
        assert_eq!(formula.name, "terraform");
        assert_eq!(formula.versions.stable, "1.10.0");
        assert_eq!(formula.revision, 1);
        assert_eq!(formula.bottle.stable.rebuild, 2);
        assert_eq!(formula.dependencies, vec!["openssl@3".to_string()]);
        assert!(formula.bottle.stable.files.contains_key("arm64_sonoma"));
        assert!(formula.bottle.stable.files.contains_key("x86_64_linux"));
    }

    #[test]
    fn defaults_to_ghcr_root_url_when_missing() {
        let source = r#"
class Terraform < Formula
  bottle do
    sha256 arm64_sonoma: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  end
end
"#;

        let spec = TapFormulaRef {
            owner: "hashicorp".to_string(),
            repo: "tap".to_string(),
            formula: "terraform".to_string(),
        };

        let formula = parse_tap_formula_ruby(&spec, source).unwrap();
        let url = &formula.bottle.stable.files["arm64_sonoma"].url;
        assert_eq!(
            url,
            "https://ghcr.io/v2/hashicorp/tap/terraform/blobs/sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn builds_release_style_bottle_url() {
        let source = r#"
class Ttfb < Formula
  version "1.3.0"
  bottle do
    root_url "https://github.com/messense/homebrew-tap/releases/download/ttfb-1.3.0"
    sha256 x86_64_linux: "054859a821b01d3dd7236e71fbf106f7a694ded54ae6aaaed221b59d3b554c42"
  end
end
"#;
        let spec = TapFormulaRef {
            owner: "messense".to_string(),
            repo: "tap".to_string(),
            formula: "ttfb".to_string(),
        };
        let formula = parse_tap_formula_ruby(&spec, source).unwrap();
        let url = &formula.bottle.stable.files["x86_64_linux"].url;
        assert_eq!(
            url,
            "https://github.com/messense/homebrew-tap/releases/download/ttfb-1.3.0/ttfb-1.3.0.x86_64_linux.bottle.tar.gz"
        );
    }

    #[test]
    fn infers_version_from_url_when_version_field_missing() {
        let source = r#"
class Jaso < Formula
  url "https://github.com/cr0sh/jaso/archive/refs/tags/v1.0.1.tar.gz"
  bottle do
    root_url "https://github.com/simnalamburt/homebrew-x/releases/download/jaso-1.0.1"
    sha256 x86_64_linux: "76c0ea0751627a7aac5495c460eecd8a7823c86e5e55b078b5884056efa8ae7f"
  end
end
"#;
        let spec = TapFormulaRef {
            owner: "simnalamburt".to_string(),
            repo: "x".to_string(),
            formula: "jaso".to_string(),
        };
        let formula = parse_tap_formula_ruby(&spec, source).unwrap();
        assert_eq!(formula.versions.stable, "1.0.1");
        assert_eq!(
            formula.bottle.stable.files["x86_64_linux"].url,
            "https://github.com/simnalamburt/homebrew-x/releases/download/jaso-1.0.1/jaso-1.0.1.x86_64_linux.bottle.tar.gz"
        );
    }

    #[test]
    fn parses_bottle_block_with_nested_do_end_sections() {
        let source = r#"
class Terraform < Formula
  version "1.10.0"
  bottle do
    root_url "https://ghcr.io/v2/hashicorp/tap"
    on_linux do
      sha256 x86_64_linux: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    end
    on_macos do
      sha256 arm64_sonoma: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    end
  end
end
"#;

        let spec = TapFormulaRef {
            owner: "hashicorp".to_string(),
            repo: "tap".to_string(),
            formula: "terraform".to_string(),
        };
        let formula = parse_tap_formula_ruby(&spec, source).unwrap();

        assert!(formula.bottle.stable.files.contains_key("x86_64_linux"));
        assert!(formula.bottle.stable.files.contains_key("arm64_sonoma"));
    }
}
