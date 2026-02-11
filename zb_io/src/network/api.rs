use crate::network::cache::{ApiCache, CacheEntry};
use crate::network::tap_formula::{parse_tap_formula_ref, parse_tap_formula_ruby};
use zb_core::{Error, Formula};

pub struct ApiClient {
    base_url: String,
    cask_base_url: String,
    tap_raw_base_url: String,
    client: reqwest::Client,
    cache: Option<ApiCache>,
}

impl ApiClient {
    pub fn new() -> Self {
        Self::with_base_url("https://formulae.brew.sh/api/formula".to_string())
    }

    pub fn with_base_url(base_url: String) -> Self {
        // Use HTTP/2 with connection pooling for better multiplexing of parallel requests
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

    pub async fn fetch_formula_rb(
        &self,
        ruby_source_path: &str,
        cache_dir: &std::path::Path,
    ) -> Result<std::path::PathBuf, Error> {
        let url = format!(
            "https://raw.githubusercontent.com/Homebrew/homebrew-core/master/{ruby_source_path}"
        );

        let cache_key = format!("rb:{url}");
        if let Some(entry) = self.cache.as_ref().and_then(|c| c.get(&cache_key)) {
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
            .get(&url)
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

    pub async fn get_formula(&self, name: &str) -> Result<Formula, Error> {
        if let Some(spec) = parse_tap_formula_ref(name) {
            return self.get_tap_formula(&spec).await;
        }

        let url = format!("{}/{}.json", self.base_url, name);

        let cached_entry = self.cache.as_ref().and_then(|c| c.get(&url));

        let mut request = self.client.get(&url);

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
            let formula: Formula =
                serde_json::from_str(&entry.body).map_err(|e| Error::NetworkFailure {
                    message: format!("failed to parse cached formula JSON: {e}"),
                })?;
            return Ok(formula);
        }

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

        if let Some(ref cache) = self.cache {
            let entry = CacheEntry {
                etag,
                last_modified,
                body: body.clone(),
            };
            let _ = cache.put(&url, &entry);
        }

        let formula: Formula = serde_json::from_str(&body).map_err(|e| Error::NetworkFailure {
            message: format!("failed to parse formula JSON: {e}"),
        })?;

        Ok(formula)
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
        let candidate_paths = [
            format!("Formula/{}.rb", spec.formula),
            format!(
                "Formula/{}/{}.rb",
                spec.formula.chars().next().unwrap_or('x'),
                spec.formula
            ),
        ];
        let branches = ["main", "master"];

        let mut last_status: Option<reqwest::StatusCode> = None;
        let mut last_network_error: Option<Error> = None;

        for repo in candidate_repos {
            for branch in branches {
                let first_url = format!(
                    "{}/{}/{}/{}/{}",
                    self.tap_raw_base_url.trim_end_matches('/'),
                    spec.owner,
                    repo,
                    branch,
                    candidate_paths[0]
                );
                let second_url = format!(
                    "{}/{}/{}/{}/{}",
                    self.tap_raw_base_url.trim_end_matches('/'),
                    spec.owner,
                    repo,
                    branch,
                    candidate_paths[1]
                );

                let (first_response, second_response) = tokio::join!(
                    self.client.get(&first_url).send(),
                    self.client.get(&second_url).send()
                );

                for response in [first_response, second_response] {
                    match response {
                        Ok(response) => {
                            if response.status().is_success() {
                                let body =
                                    response.text().await.map_err(|e| Error::NetworkFailure {
                                        message: format!("failed to read tap formula body: {e}"),
                                    })?;
                                return parse_tap_formula_ruby(spec, &body);
                            }

                            last_status = Some(response.status());
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

        if last_status == Some(reqwest::StatusCode::NOT_FOUND) {
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
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn fetches_formula_from_mock_server() {
        let mock_server = MockServer::start().await;

        let fixture = include_str!("../../../zb_core/fixtures/formula_foo.json");

        Mock::given(method("GET"))
            .and(path("/foo.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(mock_server.uri());
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

        let client = ApiClient::with_base_url(mock_server.uri());
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
        let client = ApiClient::with_base_url(mock_server.uri()).with_cache(cache);

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
        let client = ApiClient::with_base_url(mock_server.uri()).with_cache(cache);

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
        let client = ApiClient::with_base_url(mock_server.uri()).with_cache(cache);

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

        let client =
            ApiClient::with_base_url(mock_server.uri()).with_tap_raw_base_url(mock_server.uri());
        let formula = client.get_formula("hashicorp/tap/terraform").await.unwrap();

        assert_eq!(formula.name, "terraform");
        assert_eq!(formula.versions.stable, "1.10.0");
        assert!(formula.dependencies.contains(&"go".to_string()));
        assert!(formula.bottle.stable.files.contains_key("arm64_sonoma"));
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

        let client =
            ApiClient::with_base_url(mock_server.uri()).with_tap_raw_base_url(mock_server.uri());
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

        let client =
            ApiClient::with_base_url(mock_server.uri()).with_tap_raw_base_url(mock_server.uri());
        let formula = client.get_formula("hashicorp/tap/terraform").await.unwrap();

        assert_eq!(formula.name, "terraform");
        assert_eq!(formula.versions.stable, "1.10.0");
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

        let client =
            ApiClient::with_base_url(mock_server.uri()).with_cask_base_url(mock_server.uri());
        let cask = client.get_cask("iterm2").await.unwrap();
        assert_eq!(cask["token"], "iterm2");
        assert_eq!(cask["version"], "3.5.0");
    }
}
