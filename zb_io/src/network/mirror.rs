use reqwest::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomebrewApiBases {
    pub formula_base_url: String,
    pub cask_base_url: String,
}

const DEFAULT_HOMEBREW_API_DOMAIN: &str = "https://formulae.brew.sh/api";

pub fn homebrew_api_bases_from_env() -> HomebrewApiBases {
    let api_domain = std::env::var("HOMEBREW_API_DOMAIN")
        .ok()
        .filter(|value| !value.trim().is_empty());

    homebrew_api_bases_from(api_domain.as_deref())
}

pub fn homebrew_api_bases_from(api_domain: Option<&str>) -> HomebrewApiBases {
    let domain = api_domain
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_HOMEBREW_API_DOMAIN)
        .trim_end_matches('/');

    HomebrewApiBases {
        formula_base_url: format!("{domain}/formula"),
        cask_base_url: format!("{domain}/cask"),
    }
}

pub fn rewrite_bottle_url(primary_url: &str, bottle_domain: &str) -> Option<String> {
    let original = Url::parse(primary_url).ok()?;
    if original.scheme() != "http" && original.scheme() != "https" {
        return None;
    }

    let mirror = parse_mirror_prefix(bottle_domain, original.scheme())?;
    let mirror_prefix = mirror.as_str().trim_end_matches('/');

    let mut rewritten = String::with_capacity(
        mirror_prefix.len() + original.path().len() + original.query().map_or(0, |q| q.len() + 1),
    );
    rewritten.push_str(mirror_prefix);
    rewritten.push_str(original.path());

    if let Some(query) = original.query() {
        rewritten.push('?');
        rewritten.push_str(query);
    }

    if let Some(fragment) = original.fragment() {
        rewritten.push('#');
        rewritten.push_str(fragment);
    }

    Some(rewritten)
}

fn parse_mirror_prefix(mirror: &str, default_scheme: &str) -> Option<Url> {
    let trimmed = mirror.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(url) = Url::parse(trimmed) {
        return Some(url);
    }

    Url::parse(&format!("{default_scheme}://{trimmed}")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homebrew_api_bases_from_env_uses_default_when_unset() {
        let bases = homebrew_api_bases_from(None);

        assert_eq!(
            bases.formula_base_url,
            "https://formulae.brew.sh/api/formula"
        );
        assert_eq!(bases.cask_base_url, "https://formulae.brew.sh/api/cask");
    }

    #[test]
    fn homebrew_api_bases_from_env_appends_formula_and_cask_paths() {
        let bases =
            homebrew_api_bases_from(Some("https://mirrors.ustc.edu.cn/homebrew-bottles/api"));

        assert_eq!(
            bases.formula_base_url,
            "https://mirrors.ustc.edu.cn/homebrew-bottles/api/formula"
        );
        assert_eq!(
            bases.cask_base_url,
            "https://mirrors.ustc.edu.cn/homebrew-bottles/api/cask"
        );
    }

    #[test]
    fn rewrite_bottle_url_preserves_path_query_and_fragment() {
        let url = "https://ghcr.io/v2/homebrew/core/gettext/blobs/sha256:abc?foo=bar#frag";
        let mirrored = rewrite_bottle_url(url, "https://mirrors.ustc.edu.cn/homebrew-bottles")
            .expect("mirror rewrite");

        assert_eq!(
            mirrored,
            "https://mirrors.ustc.edu.cn/homebrew-bottles/v2/homebrew/core/gettext/blobs/sha256:abc?foo=bar#frag"
        );
    }

    #[test]
    fn rewrite_bottle_url_accepts_mirror_prefix_without_scheme() {
        let url = "https://ghcr.io/v2/homebrew/core/gettext/blobs/sha256:abc";
        let mirrored = rewrite_bottle_url(url, "mirrors.ustc.edu.cn/homebrew-bottles")
            .expect("mirror rewrite");

        assert_eq!(
            mirrored,
            "https://mirrors.ustc.edu.cn/homebrew-bottles/v2/homebrew/core/gettext/blobs/sha256:abc"
        );
    }
}
