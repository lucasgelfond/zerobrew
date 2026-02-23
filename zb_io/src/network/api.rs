use crate::checksum::verify_sha256_bytes;
use crate::network::cache::{ApiCache, CacheEntry};
use crate::network::tap_formula::{parse_tap_formula_ref, parse_tap_formula_ruby};
use futures_util::stream::{self, StreamExt};
use zb_core::{Error, Formula};

const HOMEBREW_CORE_RAW_BASE: &str =
    "https://raw.githubusercontent.com/Homebrew/homebrew-core/main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RubySourceLocator<'a> {
    CoreRelativePath(&'a str),
    AbsoluteUrl(&'a str),
    TapEncodedUrl(&'a str),
}

impl<'a> RubySourceLocator<'a> {
    const TAP_URL_PREFIX: &'static str = "tap-rb-url:";

    fn parse(input: &'a str) -> Self {
        if let Some(encoded_url) = input.strip_prefix(Self::TAP_URL_PREFIX) {
            return Self::TapEncodedUrl(encoded_url);
        }

        if input.starts_with("https://") || input.starts_with("http://") {
            return Self::AbsoluteUrl(input);
        }

        Self::CoreRelativePath(input)
    }

    fn source_id(self, original: &'a str) -> &'a str {
        match self {
            Self::CoreRelativePath(_) => original,
            Self::AbsoluteUrl(url) => url,
            Self::TapEncodedUrl(url) => url,
        }
    }

    fn to_url(self) -> String {
        match self {
            Self::CoreRelativePath(path) => format!("{HOMEBREW_CORE_RAW_BASE}/{path}"),
            Self::AbsoluteUrl(url) | Self::TapEncodedUrl(url) => url.to_string(),
        }
    }

    fn encode_tap_url(url: &str) -> String {
        format!("{}{}", Self::TAP_URL_PREFIX, url)
    }
}

enum CachedGetResult {
    Cached(String),
    Fresh(reqwest::Response),
}

#[derive(Debug)]
pub struct ApiClient {
    base_url: String,
    cask_base_url: String,
    tap_raw_base_url: String,
    client: reqwest::Client,
    cache: Option<ApiCache>,
}

impl ApiClient {
    const DEFAULT_BASE_URL: &'static str = "https://formulae.brew.sh/api/formula";

    pub fn new() -> Self {
        Self::build_client(Self::DEFAULT_BASE_URL.to_string())
    }

    /// Rejects non-http(s) schemes and URLs containing credentials.
    pub fn with_base_url(base_url: String) -> Result<Self, Error> {
        let parsed = reqwest::Url::parse(&base_url).map_err(|e| Error::InvalidArgument {
            message: format!("invalid API base URL: {e}"),
        })?;
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(Error::InvalidArgument {
                message: format!(
                    "API base URL must use http or https scheme, got: {}",
                    parsed.scheme()
                ),
            });
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(Error::InvalidArgument {
                message: "Bad ZEROBREW_API_URL configuration".to_string(),
            });
        }

        Ok(Self::build_client(base_url))
    }

    fn build_client(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("zerobrew/0.1")
            .pool_max_idle_per_host(20)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            base_url,
            cask_base_url: "https://formulae.brew.sh/api/cask".to_string(),
            tap_raw_base_url: "https://raw.githubusercontent.com".to_string(),
            client,
            cache: None,
        }
    }

    #[cfg(test)]
    pub fn with_tap_raw_base_url(mut self, tap_raw_base_url: String) -> Self {
        self.tap_raw_base_url = tap_raw_base_url;
        self
    }

    #[cfg(test)]
    pub fn with_cask_base_url(mut self, cask_base_url: String) -> Self {
        self.cask_base_url = cask_base_url;
        self
    }

    pub fn with_cache(mut self, cache: ApiCache) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Clear all cached API responses. Returns the number removed.
    pub fn clear_cache(&self) -> Result<usize, Error> {
        match &self.cache {
            Some(cache) => cache.clear().map_err(|e| Error::StoreCorruption {
                message: format!("failed to clear API cache: {e}"),
            }),
            None => Ok(0),
        }
    }

    pub async fn fetch_formula_rb(
        &self,
        ruby_source_path: &str,
        cache_dir: &std::path::Path,
        expected_sha256: Option<&str>,
    ) -> Result<std::path::PathBuf, Error> {
        let locator = RubySourceLocator::parse(ruby_source_path);
        let source_id = locator.source_id(ruby_source_path);
        let url = locator.to_url();

        self.fetch_formula_rb_from_url(source_id, &url, cache_dir, expected_sha256)
            .await
    }

    async fn fetch_formula_rb_from_url(
        &self,
        ruby_source_path: &str,
        url: &str,
        cache_dir: &std::path::Path,
        expected_sha256: Option<&str>,
    ) -> Result<std::path::PathBuf, Error> {
        let cache_key = format!("rb:{url}");
        if let Some(entry) = self.cache.as_ref().and_then(|c| c.get(&cache_key)) {
            verify_sha256_bytes(entry.body.as_bytes(), expected_sha256)
                .map_err(|e| Self::map_formula_rb_checksum_error(e, ruby_source_path, "cache"))?;

            let dest = cache_dir.join(ruby_source_path.replace('/', "_"));
            std::fs::create_dir_all(cache_dir).map_err(|e| Error::FileError {
                message: format!("failed to create rb cache dir: {e}"),
            })?;
            std::fs::write(&dest, entry.body.as_bytes()).map_err(|e| Error::FileError {
                message: format!("failed to write cached rb file: {e}"),
            })?;
            return Ok(dest);
        }

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| Error::NetworkFailure {
                message: format!("failed to fetch formula rb: {e}"),
            })?;

        if !response.status().is_success() {
            return Err(Error::NetworkFailure {
                message: format!("formula rb fetch returned HTTP {}", response.status()),
            });
        }

        let body = response.text().await.map_err(|e| Error::NetworkFailure {
            message: format!("failed to read formula rb response: {e}"),
        })?;

        verify_sha256_bytes(body.as_bytes(), expected_sha256)
            .map_err(|e| Self::map_formula_rb_checksum_error(e, ruby_source_path, "network"))?;

        if let Some(ref cache) = self.cache {
            let entry = CacheEntry {
                etag: None,
                last_modified: None,
                body: body.clone(),
            };
            let _ = cache.put(&cache_key, &entry);
        }

        let dest = cache_dir.join(ruby_source_path.replace('/', "_"));
        std::fs::create_dir_all(cache_dir).map_err(|e| Error::FileError {
            message: format!("failed to create rb cache dir: {e}"),
        })?;
        std::fs::write(&dest, body.as_bytes()).map_err(|e| Error::FileError {
            message: format!("failed to write rb file: {e}"),
        })?;

        Ok(dest)
    }

    fn map_formula_rb_checksum_error(err: Error, ruby_source_path: &str, source: &str) -> Error {
        match err {
            Error::ChecksumMismatch { .. } => err,
            Error::InvalidArgument { message } => Error::InvalidArgument {
                message: format!(
                    "invalid ruby_source_checksum for '{ruby_source_path}' (source: {source}): {message}"
                ),
            },
            other => other,
        }
    }

    async fn cached_get(&self, url: &str) -> Result<CachedGetResult, Error> {
        let cached_entry = self.cache.as_ref().and_then(|c| c.get(url));

        let mut request = self.client.get(url);

        if let Some(ref entry) = cached_entry {
            if let Some(ref etag) = entry.etag {
                request = request.header("If-None-Match", etag.as_str());
            }
            if let Some(ref last_modified) = entry.last_modified {
                request = request.header("If-Modified-Since", last_modified.as_str());
            }
        }

        let response = request.send().await.map_err(|e| Error::NetworkFailure {
            message: e.to_string(),
        })?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED
            && let Some(entry) = cached_entry
        {
            return Ok(CachedGetResult::Cached(entry.body));
        }

        Ok(CachedGetResult::Fresh(response))
    }

    fn store_response_in_cache(
        &self,
        url: &str,
        etag: Option<String>,
        last_modified: Option<String>,
        body: &str,
    ) {
        if let Some(ref cache) = self.cache {
            let entry = CacheEntry {
                etag,
                last_modified,
                body: body.to_string(),
            };
            let _ = cache.put(url, &entry);
        }
    }

    pub async fn get_formula(&self, name: &str) -> Result<Formula, Error> {
        if let Some(spec) = parse_tap_formula_ref(name) {
            return self.get_tap_formula(&spec).await;
        }

        let url = format!("{}/{}.json", self.base_url, name);

        let body = match self.cached_get(&url).await? {
            CachedGetResult::Cached(body) => body,
            CachedGetResult::Fresh(response) => {
                if response.status() == reqwest::StatusCode::NOT_FOUND {
                    return Err(Error::MissingFormula {
                        name: name.to_string(),
                    });
                }
                if !response.status().is_success() {
                    return Err(Error::NetworkFailure {
                        message: format!("HTTP {}", response.status()),
                    });
                }

                let etag = response
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                let last_modified = response
                    .headers()
                    .get("last-modified")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());

                let body = response.text().await.map_err(|e| Error::NetworkFailure {
                    message: format!("failed to read response body: {e}"),
                })?;

                self.store_response_in_cache(&url, etag, last_modified, &body);
                body
            }
        };

        serde_json::from_str(&body).map_err(|e| Error::NetworkFailure {
            message: format!("failed to parse formula JSON: {e}"),
        })
    }

    pub async fn get_all_formulas_raw(&self) -> Result<String, Error> {
        let url = format!("{}.json", self.base_url);

        match self.cached_get(&url).await? {
            CachedGetResult::Cached(body) => Ok(body),
            CachedGetResult::Fresh(response) => {
                if !response.status().is_success() {
                    return Err(Error::NetworkFailure {
                        message: format!("bulk formula fetch returned HTTP {}", response.status()),
                    });
                }

                let etag = response
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                let last_modified = response
                    .headers()
                    .get("last-modified")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());

                let body = response.text().await.map_err(|e| Error::NetworkFailure {
                    message: format!("failed to read bulk formula response body: {e}"),
                })?;

                self.store_response_in_cache(&url, etag, last_modified, &body);
                Ok(body)
            }
        }
    }

    pub async fn get_cask(&self, token: &str) -> Result<serde_json::Value, Error> {
        let url = format!("{}/{}.json", self.cask_base_url, token);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::NetworkFailure {
                message: e.to_string(),
            })?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(Error::MissingFormula {
                name: format!("cask:{token}"),
            });
        }

        if !response.status().is_success() {
            return Err(Error::NetworkFailure {
                message: format!("HTTP {}", response.status()),
            });
        }

        response
            .json::<serde_json::Value>()
            .await
            .map_err(|e| Error::NetworkFailure {
                message: format!("failed to parse cask JSON: {e}"),
            })
    }

    async fn get_tap_formula(
        &self,
        spec: &crate::network::tap_formula::TapFormulaRef,
    ) -> Result<Formula, Error> {
        let candidate_repos = if spec.repo.starts_with("homebrew-") {
            vec![
                spec.repo.clone(),
                spec.repo.trim_start_matches("homebrew-").to_string(),
            ]
        } else {
            vec![format!("homebrew-{}", spec.repo), spec.repo.clone()]
        };
        let first_char = spec.formula.chars().next().unwrap_or('x');
        let candidate_paths = [
            format!("Formula/{}.rb", spec.formula),
            format!("Formula/{first_char}/{}.rb", spec.formula),
            format!("HomebrewFormula/{}.rb", spec.formula),
            format!("HomebrewFormula/{first_char}/{}.rb", spec.formula),
            format!("{}.rb", spec.formula),
        ];
        let branches = ["main", "master"];

        let mut last_status: Option<reqwest::StatusCode> = None;
        let mut last_network_error: Option<Error> = None;
        let mut saw_non_404_status = false;

        for repo in candidate_repos {
            for branch in branches {
                let base_prefix = format!(
                    "{}/{}/{}/{}/",
                    self.tap_raw_base_url.trim_end_matches('/'),
                    spec.owner,
                    repo,
                    branch,
                );
                let client = self.client.clone();
                let mut responses = stream::iter(candidate_paths.iter().map(|candidate_path| {
                    let client = client.clone();
                    let url = format!("{base_prefix}{candidate_path}");
                    async move { (url.clone(), client.get(&url).send().await) }
                }))
                .buffered(2);

                while let Some((url, response)) = responses.next().await {
                    match response {
                        Ok(response) => {
                            let status = response.status();
                            if status.is_success() {
                                let body =
                                    response.text().await.map_err(|e| Error::NetworkFailure {
                                        message: format!("failed to read tap formula body: {e}"),
                                    })?;
                                let mut formula = parse_tap_formula_ruby(spec, &body)?;
                                formula.ruby_source_path =
                                    Some(RubySourceLocator::encode_tap_url(&url));
                                return Ok(formula);
                            }

                            if status != reqwest::StatusCode::NOT_FOUND {
                                saw_non_404_status = true;
                            }
                            last_status = Some(status);
                        }
                        Err(e) => {
                            last_network_error = Some(Error::NetworkFailure {
                                message: e.to_string(),
                            });
                        }
                    }
                }
            }
        }

        if !saw_non_404_status
            && last_network_error.is_none()
            && last_status == Some(reqwest::StatusCode::NOT_FOUND)
        {
            return Err(Error::MissingFormula {
                name: format!("{}/{}/{}", spec.owner, spec.repo, spec.formula),
            });
        }

        if let Some(err) = last_network_error {
            return Err(err);
        }

        Err(Error::NetworkFailure {
            message: format!(
                "failed to fetch tap formula '{}/{}/{}' (last status: {})",
                spec.owner,
                spec.repo,
                spec.formula,
                last_status
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
        })
    }
}

impl Default for ApiClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn with_base_url_rejects_non_http_schemes() {
        let err = ApiClient::with_base_url("ftp://example.com/api".into()).unwrap_err();
        assert!(matches!(err, Error::InvalidArgument { .. }));
        assert!(err.to_string().contains("http or https"));
    }

    #[test]
    fn with_base_url_rejects_embedded_credentials() {
        let err = ApiClient::with_base_url("https://user:pass@example.com/api".into()).unwrap_err();
        assert!(matches!(err, Error::InvalidArgument { .. }));
    }

    #[test]
    fn with_base_url_rejects_garbage() {
        assert!(ApiClient::with_base_url("not a url".into()).is_err());
    }

    #[test]
    fn with_base_url_accepts_valid_https() {
        assert!(ApiClient::with_base_url("https://mirror.example.com/api/formula".into()).is_ok());
    }

    #[test]
    fn with_base_url_accepts_valid_http() {
        assert!(ApiClient::with_base_url("http://localhost:8080/api".into()).is_ok());
    }

    #[test]
    fn ruby_source_locator_parses_all_supported_kinds() {
        assert_eq!(
            RubySourceLocator::parse("Formula/f/foo.rb"),
            RubySourceLocator::CoreRelativePath("Formula/f/foo.rb")
        );
        assert_eq!(
            RubySourceLocator::parse("https://example.com/foo.rb"),
            RubySourceLocator::AbsoluteUrl("https://example.com/foo.rb")
        );
        let encoded = format!(
            "{}{}",
            RubySourceLocator::TAP_URL_PREFIX,
            "https://example.com/tap/foo.rb"
        );
        assert_eq!(
            RubySourceLocator::parse(&encoded),
            RubySourceLocator::TapEncodedUrl("https://example.com/tap/foo.rb")
        );
    }

    #[test]
    fn ruby_source_locator_resolves_urls_exhaustively() {
        assert_eq!(
            RubySourceLocator::CoreRelativePath("Formula/f/foo.rb").to_url(),
            "https://raw.githubusercontent.com/Homebrew/homebrew-core/main/Formula/f/foo.rb"
        );
        assert_eq!(
            RubySourceLocator::AbsoluteUrl("https://example.com/foo.rb").to_url(),
            "https://example.com/foo.rb"
        );
        assert_eq!(
            RubySourceLocator::TapEncodedUrl(
                "https://raw.githubusercontent.com/org/tap/main/foo.rb"
            )
            .to_url(),
            "https://raw.githubusercontent.com/org/tap/main/foo.rb"
        );
    }

    #[tokio::test]
    async fn fetches_formula_from_mock_server() {
        let mock_server = MockServer::start().await;

        let fixture = include_str!("../../../zb_core/fixtures/formula_foo.json");

        Mock::given(method("GET"))
            .and(path("/foo.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(mock_server.uri()).unwrap();
        let formula = client.get_formula("foo").await.unwrap();

        assert_eq!(formula.name, "foo");
        assert_eq!(formula.versions.stable, "1.2.3");
    }

    #[tokio::test]
    async fn returns_missing_formula_on_404() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/nonexistent.json"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(mock_server.uri()).unwrap();
        let err = client.get_formula("nonexistent").await.unwrap_err();

        assert!(matches!(
            err,
            Error::MissingFormula { name } if name == "nonexistent"
        ));
    }

    #[tokio::test]
    async fn first_request_stores_etag() {
        let mock_server = MockServer::start().await;
        let fixture = include_str!("../../../zb_core/fixtures/formula_foo.json");

        Mock::given(method("GET"))
            .and(path("/foo.json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(fixture)
                    .insert_header("etag", "\"abc123\""),
            )
            .mount(&mock_server)
            .await;

        let cache = ApiCache::in_memory().unwrap();
        let client = ApiClient::with_base_url(mock_server.uri())
            .unwrap()
            .with_cache(cache);

        let _ = client.get_formula("foo").await.unwrap();

        let cached = client
            .cache
            .as_ref()
            .unwrap()
            .get(&format!("{}/foo.json", mock_server.uri()))
            .unwrap();
        assert_eq!(cached.etag, Some("\"abc123\"".to_string()));
    }

    #[tokio::test]
    async fn second_request_sends_if_none_match() {
        let mock_server = MockServer::start().await;
        let fixture = include_str!("../../../zb_core/fixtures/formula_foo.json");

        // First request returns 200 with ETag
        Mock::given(method("GET"))
            .and(path("/foo.json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(fixture)
                    .insert_header("etag", "\"abc123\""),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let cache = ApiCache::in_memory().unwrap();
        let client = ApiClient::with_base_url(mock_server.uri())
            .unwrap()
            .with_cache(cache);

        // First request
        let _ = client.get_formula("foo").await.unwrap();

        // Reset mocks for second request
        mock_server.reset().await;

        // Second request should send If-None-Match and receive 304
        Mock::given(method("GET"))
            .and(path("/foo.json"))
            .and(header("If-None-Match", "\"abc123\""))
            .respond_with(ResponseTemplate::new(304))
            .expect(1)
            .mount(&mock_server)
            .await;

        let formula = client.get_formula("foo").await.unwrap();
        assert_eq!(formula.name, "foo");
    }

    #[tokio::test]
    async fn uses_cached_body_on_304() {
        let mock_server = MockServer::start().await;
        let fixture = include_str!("../../../zb_core/fixtures/formula_foo.json");

        // First request returns 200 with ETag
        Mock::given(method("GET"))
            .and(path("/foo.json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(fixture)
                    .insert_header("etag", "\"abc123\""),
            )
            .mount(&mock_server)
            .await;

        let cache = ApiCache::in_memory().unwrap();
        let client = ApiClient::with_base_url(mock_server.uri())
            .unwrap()
            .with_cache(cache);

        // First request populates cache
        let _ = client.get_formula("foo").await.unwrap();

        mock_server.reset().await;

        // Second request returns 304 (no body)
        Mock::given(method("GET"))
            .and(path("/foo.json"))
            .and(header("If-None-Match", "\"abc123\""))
            .respond_with(ResponseTemplate::new(304))
            .mount(&mock_server)
            .await;

        // Should return cached formula
        let formula = client.get_formula("foo").await.unwrap();
        assert_eq!(formula.name, "foo");
        assert_eq!(formula.versions.stable, "1.2.3");
    }

    #[tokio::test]
    async fn fetches_formula_from_tap_ruby_source() {
        let mock_server = MockServer::start().await;
        let rb = r#"
class Terraform < Formula
  version "1.10.0"
  depends_on "go"
  bottle do
    root_url "https://ghcr.io/v2/hashicorp/tap"
    sha256 arm64_sonoma: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  end
end
"#;

        Mock::given(method("GET"))
            .and(path("/hashicorp/homebrew-tap/main/Formula/terraform.rb"))
            .respond_with(ResponseTemplate::new(200).set_body_string(rb))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(mock_server.uri())
            .unwrap()
            .with_tap_raw_base_url(mock_server.uri());
        let formula = client.get_formula("hashicorp/tap/terraform").await.unwrap();

        assert_eq!(formula.name, "terraform");
        assert_eq!(formula.versions.stable, "1.10.0");
        assert!(formula.dependencies.contains(&"go".to_string()));
        assert!(formula.bottle.stable.files.contains_key("arm64_sonoma"));
        let expected_path = format!(
            "{}{}/hashicorp/homebrew-tap/main/Formula/terraform.rb",
            RubySourceLocator::TAP_URL_PREFIX,
            mock_server.uri()
        );
        assert_eq!(
            formula.ruby_source_path.as_deref(),
            Some(expected_path.as_str())
        );
    }

    #[tokio::test]
    async fn supports_source_only_tap_formula_without_bottle_block() {
        let mock_server = MockServer::start().await;
        let rb = r#"
class OhMyPosh < Formula
  version "29.3.0"
  url "https://github.com/JanDeDobbeleer/oh-my-posh/archive/v29.3.0.tar.gz"
  sha256 "ff39f6ef2b4ca2d7d766f2802520b023986a5d6dbcd59fba685a9e5bacf41993"
  depends_on "go@1.26" => :build
end
"#;

        Mock::given(method("GET"))
            .and(path(
                "/jandedobbeleer/homebrew-oh-my-posh/main/oh-my-posh.rb",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(rb))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(mock_server.uri())
            .unwrap()
            .with_tap_raw_base_url(mock_server.uri());
        let formula = client
            .get_formula("jandedobbeleer/oh-my-posh/oh-my-posh")
            .await
            .unwrap();

        assert_eq!(formula.name, "oh-my-posh");
        assert!(formula.bottle.stable.files.is_empty());
        assert_eq!(formula.build_dependencies, vec!["go@1.26".to_string()]);
        assert!(formula.has_source_url());
        assert!(
            formula
                .ruby_source_path
                .as_deref()
                .is_some_and(|path| path.starts_with(RubySourceLocator::TAP_URL_PREFIX))
        );
    }

    #[tokio::test]
    async fn falls_back_to_master_when_main_missing_for_tap_formula() {
        let mock_server = MockServer::start().await;
        let rb = r#"
class Terraform < Formula
  version "1.10.0"
  bottle do
    root_url "https://ghcr.io/v2/hashicorp/tap"
    sha256 arm64_sonoma: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  end
end
"#;

        Mock::given(method("GET"))
            .and(path("/hashicorp/homebrew-tap/main/Formula/terraform.rb"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/hashicorp/homebrew-tap/master/Formula/terraform.rb"))
            .respond_with(ResponseTemplate::new(200).set_body_string(rb))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(mock_server.uri())
            .unwrap()
            .with_tap_raw_base_url(mock_server.uri());
        let formula = client.get_formula("hashicorp/tap/terraform").await.unwrap();

        assert_eq!(formula.name, "terraform");
        assert_eq!(formula.versions.stable, "1.10.0");
    }

    #[tokio::test]
    async fn resolves_tap_formula_from_letter_subdirectory_path() {
        let mock_server = MockServer::start().await;
        let rb = r#"
class Terraform < Formula
  version "1.10.0"
  bottle do
    root_url "https://ghcr.io/v2/hashicorp/tap"
    sha256 arm64_sonoma: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  end
end
"#;

        Mock::given(method("GET"))
            .and(path("/hashicorp/homebrew-tap/main/Formula/t/terraform.rb"))
            .respond_with(ResponseTemplate::new(200).set_body_string(rb))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(mock_server.uri())
            .unwrap()
            .with_tap_raw_base_url(mock_server.uri());
        let formula = client.get_formula("hashicorp/tap/terraform").await.unwrap();

        assert_eq!(formula.name, "terraform");
        assert_eq!(formula.versions.stable, "1.10.0");
    }

    #[tokio::test]
    async fn resolves_tap_formula_from_homebrewformula_directory() {
        let mock_server = MockServer::start().await;
        let rb = r#"
class Terraform < Formula
  version "1.10.0"
  bottle do
    root_url "https://ghcr.io/v2/hashicorp/tap"
    sha256 arm64_sonoma: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  end
end
"#;

        Mock::given(method("GET"))
            .and(path(
                "/hashicorp/homebrew-tap/main/HomebrewFormula/terraform.rb",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(rb))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(mock_server.uri())
            .unwrap()
            .with_tap_raw_base_url(mock_server.uri());
        let formula = client.get_formula("hashicorp/tap/terraform").await.unwrap();

        assert_eq!(formula.name, "terraform");
        assert_eq!(formula.versions.stable, "1.10.0");
    }

    #[tokio::test]
    async fn resolves_tap_formula_from_homebrewformula_letter_subdirectory_path() {
        let mock_server = MockServer::start().await;
        let rb = r#"
class Terraform < Formula
  version "1.10.0"
  bottle do
    root_url "https://ghcr.io/v2/hashicorp/tap"
    sha256 arm64_sonoma: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  end
end
"#;

        Mock::given(method("GET"))
            .and(path(
                "/hashicorp/homebrew-tap/main/HomebrewFormula/t/terraform.rb",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(rb))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(mock_server.uri())
            .unwrap()
            .with_tap_raw_base_url(mock_server.uri());
        let formula = client.get_formula("hashicorp/tap/terraform").await.unwrap();

        assert_eq!(formula.name, "terraform");
        assert_eq!(formula.versions.stable, "1.10.0");
    }

    #[tokio::test]
    async fn resolves_tap_formula_from_repository_root() {
        let mock_server = MockServer::start().await;
        let rb = r#"
class Terraform < Formula
  version "1.10.0"
  bottle do
    root_url "https://ghcr.io/v2/hashicorp/tap"
    sha256 arm64_sonoma: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  end
end
"#;

        Mock::given(method("GET"))
            .and(path("/hashicorp/homebrew-tap/main/terraform.rb"))
            .respond_with(ResponseTemplate::new(200).set_body_string(rb))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(mock_server.uri())
            .unwrap()
            .with_tap_raw_base_url(mock_server.uri());
        let formula = client.get_formula("hashicorp/tap/terraform").await.unwrap();

        assert_eq!(formula.name, "terraform");
        assert_eq!(formula.versions.stable, "1.10.0");
    }

    #[tokio::test]
    async fn returns_missing_formula_when_all_tap_candidates_are_404() {
        let mock_server = MockServer::start().await;

        let client = ApiClient::with_base_url(mock_server.uri())
            .unwrap()
            .with_tap_raw_base_url(mock_server.uri());
        let err = client
            .get_formula("hashicorp/tap/terraform")
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            Error::MissingFormula { name } if name == "hashicorp/tap/terraform"
        ));
    }

    #[tokio::test]
    async fn does_not_return_missing_formula_when_a_non_404_tap_status_is_seen() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/hashicorp/homebrew-tap/main/Formula/terraform.rb"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(mock_server.uri())
            .unwrap()
            .with_tap_raw_base_url(mock_server.uri());
        let err = client
            .get_formula("hashicorp/tap/terraform")
            .await
            .unwrap_err();

        assert!(matches!(err, Error::NetworkFailure { .. }));
    }

    #[tokio::test]
    async fn fetch_formula_rb_supports_absolute_url_paths() {
        let mock_server = MockServer::start().await;
        let ruby_body = "class Foo < Formula\nend\n";

        Mock::given(method("GET"))
            .and(path("/custom/foo.rb"))
            .respond_with(ResponseTemplate::new(200).set_body_string(ruby_body))
            .mount(&mock_server)
            .await;

        let cache_dir = tempdir().unwrap();
        let client = ApiClient::new();

        let fetched = client
            .fetch_formula_rb(
                &format!("{}/custom/foo.rb", mock_server.uri()),
                cache_dir.path(),
                None,
            )
            .await
            .unwrap();

        assert!(fetched.exists());
    }

    #[tokio::test]
    async fn fetch_formula_rb_from_network_rejects_checksum_mismatch() {
        let mock_server = MockServer::start().await;
        let ruby_body = "class Foo < Formula\nend\n";

        Mock::given(method("GET"))
            .and(path("/Formula/f/foo.rb"))
            .respond_with(ResponseTemplate::new(200).set_body_string(ruby_body))
            .mount(&mock_server)
            .await;

        let cache_dir = tempdir().unwrap();
        let client = ApiClient::new();

        let err = client
            .fetch_formula_rb_from_url(
                "Formula/f/foo.rb",
                &format!("{}/Formula/f/foo.rb", mock_server.uri()),
                cache_dir.path(),
                Some(&"0".repeat(64)),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, Error::ChecksumMismatch { .. }));
    }

    #[tokio::test]
    async fn fetch_formula_rb_from_cache_rejects_checksum_mismatch() {
        let cache = ApiCache::in_memory().unwrap();
        let cache_url = "https://example.invalid/Formula/f/foo.rb";
        cache
            .put(
                &format!("rb:{cache_url}"),
                &CacheEntry {
                    etag: None,
                    last_modified: None,
                    body: "class Foo < Formula\nend\n".to_string(),
                },
            )
            .unwrap();

        let cache_dir = tempdir().unwrap();
        let client = ApiClient::new().with_cache(cache);

        let err = client
            .fetch_formula_rb_from_url(
                "Formula/f/foo.rb",
                cache_url,
                cache_dir.path(),
                Some(&"f".repeat(64)),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, Error::ChecksumMismatch { .. }));
    }

    #[tokio::test]
    async fn fetches_cask_json() {
        let mock_server = MockServer::start().await;
        let cask_json = r#"{
  "token": "iterm2",
  "version": "3.5.0",
  "url": "https://example.com/iterm2.zip",
  "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "artifacts": [{"app":["iTerm.app"]}]
}"#;

        Mock::given(method("GET"))
            .and(path("/iterm2.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(cask_json))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(mock_server.uri())
            .unwrap()
            .with_cask_base_url(mock_server.uri());
        let cask = client.get_cask("iterm2").await.unwrap();
        assert_eq!(cask["token"], "iterm2");
        assert_eq!(cask["version"], "3.5.0");
    }

    #[tokio::test]
    async fn get_all_formulas_raw_returns_bulk_json() {
        let mock_server = MockServer::start().await;
        let fixture = include_str!("../../../zb_core/fixtures/formula_foo.json");
        let bulk_body = format!("[{}]", fixture);

        Mock::given(method("GET"))
            .and(path("/formula.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&bulk_body))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(format!("{}/formula", mock_server.uri())).unwrap();
        let raw = client.get_all_formulas_raw().await.unwrap();

        let formulas: Vec<Formula> = serde_json::from_str(&raw).unwrap();
        assert_eq!(formulas.len(), 1);
        assert_eq!(formulas[0].name, "foo");
        assert_eq!(formulas[0].versions.stable, "1.2.3");
    }
}
