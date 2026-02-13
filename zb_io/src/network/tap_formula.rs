use regex::Regex;
use std::collections::BTreeMap;
use std::sync::LazyLock;
use zb_core::formula::{
    Bottle, BottleFile, BottleStable, FormulaUrls, KegOnly, SourceUrl, Versions,
};
use zb_core::{Error, Formula};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TapFormulaRef {
    pub owner: String,
    pub repo: String,
    pub formula: String,
}

static VERSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^\s*version\s+["']([^"']+)["']"#).expect("VERSION_RE must compile")
});
static URL_VERSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?m)^\s*url\s+["'][^"']*(?:refs/tags|archive|download)/v?([0-9][0-9A-Za-z._+-]*)"#,
    )
    .expect("URL_VERSION_RE must compile")
});
static REVISION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^\s*revision\s+(\d+)\s*$"#).expect("REVISION_RE must compile")
});
static DEPENDS_ON_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^\s*depends_on\s+["']([^"']+)["'](.*)$"#).expect("DEPENDS_ON_RE must compile")
});
static SOURCE_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^\s*url\s+["']([^"']+)["']"#).expect("SOURCE_URL_RE must compile")
});
static SOURCE_SHA_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^\s*sha256\s+["']([0-9a-f]{64})["']\s*$"#)
        .expect("SOURCE_SHA_RE must compile")
});
static CLASS_START_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^\s*class\s+\w+\s*<\s*Formula\b"#).expect("CLASS_START_RE must compile")
});
static BOTTLE_START_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^\s*bottle\s+do\b"#).expect("BOTTLE_START_RE must compile"));
static END_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^\s*end\b"#).expect("END_RE must compile"));
static DO_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\bdo\b\s*(?:\|[^|]*\|\s*)?(?:#.*)?$"#).expect("DO_RE must compile")
});
static KEYWORD_START_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^\s*(if|unless|case|begin|def|class|module|for|while|until)\b"#)
        .expect("KEYWORD_START_RE must compile")
});
static ROOT_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"root_url\s+["']([^"']+)["']"#).expect("ROOT_URL_RE must compile")
});
static REBUILD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^\s*rebuild\s+(\d+)\s*$"#).expect("REBUILD_RE must compile")
});
static BOTTLE_SHA_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"([a-z0-9_]+):\s*"([0-9a-f]{64})""#).expect("BOTTLE_SHA_RE must compile")
});

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
    let dependencies = parse_runtime_dependencies(source);
    let build_dependencies = parse_build_dependencies(source);
    let parsed_source_url = parse_source_url(source);
    let bottle = parse_bottle(spec, source, &stable, revision);

    let source_url = match parsed_source_url {
        ParsedSourceUrl::PresentWithChecksum(source_url) => Some(source_url),
        ParsedSourceUrl::PresentMissingChecksum => {
            if bottle.is_none() {
                return Err(Error::UnsupportedFormula {
                    name: spec.formula.clone(),
                    reason: "tap formula source url is missing sha256".to_string(),
                });
            }
            None
        }
        ParsedSourceUrl::NotPresent => None,
    };

    if bottle.is_none() && source_url.is_none() {
        return Err(Error::UnsupportedFormula {
            name: spec.formula.clone(),
            reason: "tap formula does not provide bottle data or source url".to_string(),
        });
    }

    Ok(Formula {
        name: spec.formula.clone(),
        versions: Versions { stable },
        dependencies,
        bottle: bottle.unwrap_or_else(empty_bottle),
        revision,
        keg_only: KegOnly::default(),
        build_dependencies,
        urls: source_url.map(|stable| FormulaUrls {
            stable: Some(stable),
            head: None,
        }),
        ruby_source_path: None,
        ruby_source_checksum: None,
        uses_from_macos: Vec::new(),
        requirements: Vec::new(),
        variations: None,
    })
}

fn parse_version(source: &str) -> Option<String> {
    if let Some(v) = VERSION_RE
        .captures(source)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
    {
        return Some(v);
    }

    URL_VERSION_RE
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
    REVISION_RE
        .captures(source)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u32>().ok())
}

fn parse_runtime_dependencies(source: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let body = extract_formula_class_body(source).unwrap_or(source);
    let mut depth = 0usize;

    for line in body.lines() {
        let trimmed = line.trim();
        if depth == 0
            && let Some(cap) = DEPENDS_ON_RE.captures(trimmed)
        {
            let options = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            if !options.contains(":build")
                && !options.contains(":test")
                && let Some(dep) = cap.get(1)
            {
                deps.push(dep.as_str().to_string());
            }
        }
        update_depth(&mut depth, trimmed);
    }

    deps.sort_unstable();
    deps.dedup();
    deps
}

fn parse_build_dependencies(source: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let body = extract_formula_class_body(source).unwrap_or(source);
    let mut depth = 0usize;

    for line in body.lines() {
        let trimmed = line.trim();
        if depth == 0
            && let Some(cap) = DEPENDS_ON_RE.captures(trimmed)
        {
            let options = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            if options.contains(":build")
                && let Some(dep) = cap.get(1)
            {
                deps.push(dep.as_str().to_string());
            }
        }
        update_depth(&mut depth, trimmed);
    }

    deps.sort_unstable();
    deps.dedup();
    deps
}

enum ParsedSourceUrl {
    NotPresent,
    PresentMissingChecksum,
    PresentWithChecksum(SourceUrl),
}

fn parse_source_url(source: &str) -> ParsedSourceUrl {
    let body = extract_formula_class_body(source).unwrap_or(source);
    let mut depth = 0usize;
    let mut url: Option<String> = None;
    let mut checksum: Option<String> = None;

    for line in body.lines() {
        let trimmed = line.trim();

        if depth == 0 {
            if url.is_none()
                && let Some(cap) = SOURCE_URL_RE.captures(trimmed)
            {
                url = cap.get(1).map(|m| m.as_str().to_string());
            }

            if checksum.is_none()
                && let Some(cap) = SOURCE_SHA_RE.captures(trimmed)
            {
                checksum = cap.get(1).map(|m| m.as_str().to_string());
            }

            if url.is_some() && checksum.is_some() {
                break;
            }
        }

        update_depth(&mut depth, trimmed);
    }

    match (url, checksum) {
        (Some(url), Some(checksum)) => ParsedSourceUrl::PresentWithChecksum(SourceUrl {
            url,
            checksum: Some(checksum),
            tag: None,
            revision: None,
        }),
        (Some(_), None) => ParsedSourceUrl::PresentMissingChecksum,
        _ => ParsedSourceUrl::NotPresent,
    }
}

fn update_depth(depth: &mut usize, trimmed: &str) {
    if END_RE.is_match(trimmed) {
        *depth = depth.saturating_sub(1);
        return;
    }

    *depth += DO_RE.find_iter(trimmed).count();
    if KEYWORD_START_RE.is_match(trimmed) {
        *depth += 1;
    }
}

fn extract_formula_class_body(source: &str) -> Option<&str> {
    let mut offset = 0usize;
    let mut class_body_start: Option<usize> = None;
    let mut depth = 0usize;

    for line in source.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let trimmed = line.trim();

        if class_body_start.is_none() {
            if CLASS_START_RE.is_match(trimmed) {
                class_body_start = Some(offset);
                depth = 1;
            }
            continue;
        }

        let depth_before = depth;
        update_depth(&mut depth, trimmed);
        if depth_before > 0 && depth == 0 {
            return class_body_start.map(|start| &source[start..line_start]);
        }
    }

    None
}

fn parse_bottle(spec: &TapFormulaRef, source: &str, stable: &str, revision: u32) -> Option<Bottle> {
    let block = extract_bottle_block(source)?;

    let root_url = parse_root_url(block)
        .unwrap_or_else(|| format!("https://ghcr.io/v2/{}/{}", spec.owner, spec.repo));
    let rebuild = parse_rebuild(block).unwrap_or(0);
    let files = parse_bottle_files(spec, &root_url, stable, revision, rebuild, block);

    if files.is_empty() {
        return None;
    }

    Some(Bottle {
        stable: BottleStable { files, rebuild },
    })
}

fn empty_bottle() -> Bottle {
    Bottle {
        stable: BottleStable {
            files: BTreeMap::new(),
            rebuild: 0,
        },
    }
}

fn extract_bottle_block(source: &str) -> Option<&str> {
    let mut offset = 0usize;
    let mut bottle_body_start: Option<usize> = None;
    let mut depth = 0usize;

    for line in source.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let trimmed = line.trim();

        if bottle_body_start.is_none() {
            if BOTTLE_START_RE.is_match(trimmed) {
                bottle_body_start = Some(offset);
                depth = 1;
            }
            continue;
        }

        let depth_before = depth;
        update_depth(&mut depth, trimmed);
        if depth_before > 0 && depth == 0 {
            return bottle_body_start.map(|start| &source[start..line_start]);
        }
    }

    None
}

fn parse_root_url(block: &str) -> Option<String> {
    ROOT_URL_RE
        .captures(block)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn parse_rebuild(block: &str) -> Option<u32> {
    REBUILD_RE
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

    for cap in BOTTLE_SHA_RE.captures_iter(block) {
        let Some(tag) = cap.get(1).map(|m| m.as_str()) else {
            continue;
        };
        let Some(sha) = cap.get(2).map(|m| m.as_str()) else {
            continue;
        };
        if tag == "cellar" {
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
        assert_eq!(formula.build_dependencies, vec!["go".to_string()]);
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

    #[test]
    fn supports_source_only_tap_formula_without_bottle_block() {
        let source = r#"
class OhMyPosh < Formula
  version "29.3.0"
  url "https://github.com/JanDeDobbeleer/oh-my-posh/archive/v29.3.0.tar.gz"
  sha256 "ff39f6ef2b4ca2d7d766f2802520b023986a5d6dbcd59fba685a9e5bacf41993"
  depends_on "go@1.26" => :build
end
"#;

        let spec = TapFormulaRef {
            owner: "jandedobbeleer".to_string(),
            repo: "oh-my-posh".to_string(),
            formula: "oh-my-posh".to_string(),
        };

        let formula = parse_tap_formula_ruby(&spec, source).unwrap();
        assert!(formula.bottle.stable.files.is_empty());
        assert_eq!(formula.build_dependencies, vec!["go@1.26".to_string()]);

        let stable = formula
            .urls
            .as_ref()
            .and_then(|u| u.stable.as_ref())
            .expect("stable source url should be parsed");
        assert_eq!(
            stable.url,
            "https://github.com/JanDeDobbeleer/oh-my-posh/archive/v29.3.0.tar.gz"
        );
        assert_eq!(
            stable.checksum.as_deref(),
            Some("ff39f6ef2b4ca2d7d766f2802520b023986a5d6dbcd59fba685a9e5bacf41993")
        );
    }

    #[test]
    fn source_url_parsing_ignores_nested_resource_blocks() {
        let source = r#"
class Example < Formula
  url "https://example.com/example-1.0.0.tar.gz"
  sha256 "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

  resource "extra" do
    url "https://example.com/resource.tar.gz"
    sha256 "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
  end
end
"#;

        let spec = TapFormulaRef {
            owner: "someone".to_string(),
            repo: "tap".to_string(),
            formula: "example".to_string(),
        };

        let formula = parse_tap_formula_ruby(&spec, source).unwrap();
        let stable = formula
            .urls
            .as_ref()
            .and_then(|u| u.stable.as_ref())
            .expect("stable source url should be parsed");

        assert_eq!(stable.url, "https://example.com/example-1.0.0.tar.gz");
        assert_eq!(
            stable.checksum.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn source_url_without_sha256_is_unsupported() {
        let source = r#"
class Example < Formula
  url "https://example.com/example-1.0.0.tar.gz"
end
"#;

        let spec = TapFormulaRef {
            owner: "someone".to_string(),
            repo: "tap".to_string(),
            formula: "example".to_string(),
        };

        let err = parse_tap_formula_ruby(&spec, source).unwrap_err();
        assert!(matches!(
            err,
            Error::UnsupportedFormula { reason, .. }
            if reason.contains("missing sha256")
        ));
    }

    #[test]
    fn source_url_without_top_level_sha256_is_unsupported_even_if_nested_has_sha256() {
        let source = r#"
class Example < Formula
  url "https://example.com/example-1.0.0.tar.gz"

  resource "extra" do
    url "https://example.com/resource.tar.gz"
    sha256 "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
  end
end
"#;

        let spec = TapFormulaRef {
            owner: "someone".to_string(),
            repo: "tap".to_string(),
            formula: "example".to_string(),
        };

        let err = parse_tap_formula_ruby(&spec, source).unwrap_err();
        assert!(matches!(
            err,
            Error::UnsupportedFormula { reason, .. }
            if reason.contains("missing sha256")
        ));
    }

    #[test]
    fn dependency_parsing_ignores_nested_blocks() {
        let source = r#"
class Example < Formula
  url "https://example.com/example-1.0.0.tar.gz"
  sha256 "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  depends_on "openssl@3"
  depends_on "go" => :build

  resource "extra" do
    depends_on "python@3.12"
  end
end
"#;

        let spec = TapFormulaRef {
            owner: "someone".to_string(),
            repo: "tap".to_string(),
            formula: "example".to_string(),
        };

        let formula = parse_tap_formula_ruby(&spec, source).unwrap();
        assert_eq!(formula.dependencies, vec!["openssl@3".to_string()]);
        assert_eq!(formula.build_dependencies, vec!["go".to_string()]);
    }

    #[test]
    fn parser_does_not_treat_do_inside_strings_as_block_start() {
        let source = r#"
class Example < Formula
  desc "A tool to do amazing things"
  url "https://example.com/example-1.0.0.tar.gz"
  sha256 "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  depends_on "openssl@3"
  depends_on "go" => :build

  resource "extra" do |r|
    depends_on "python@3.12"
    r.url "https://example.com/resource.tar.gz"
  end
end
"#;

        let spec = TapFormulaRef {
            owner: "someone".to_string(),
            repo: "tap".to_string(),
            formula: "example".to_string(),
        };

        let formula = parse_tap_formula_ruby(&spec, source).unwrap();
        assert_eq!(formula.dependencies, vec!["openssl@3".to_string()]);
        assert_eq!(formula.build_dependencies, vec!["go".to_string()]);

        let stable = formula
            .urls
            .as_ref()
            .and_then(|u| u.stable.as_ref())
            .expect("stable source url should be parsed");
        assert_eq!(stable.url, "https://example.com/example-1.0.0.tar.gz");
        assert_eq!(
            stable.checksum.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn returns_unsupported_formula_when_neither_bottle_nor_source_is_available() {
        let source = r#"
class Terraform < Formula
  version "1.10.0"
end
"#;

        let spec = TapFormulaRef {
            owner: "hashicorp".to_string(),
            repo: "tap".to_string(),
            formula: "terraform".to_string(),
        };

        let err = parse_tap_formula_ruby(&spec, source).unwrap_err();
        assert!(matches!(err, Error::UnsupportedFormula { .. }));
    }
}
