use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, HeaderValue, WWW_AUTHENTICATE};
use serde::Deserialize;
use tokio::sync::RwLock;

use zb_core::Error;

use super::MAX_CHUNK_RETRIES;

pub(crate) fn bearer_header(token: &str) -> Result<HeaderValue, Error> {
    HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| Error::NetworkFailure {
        message: "auth token contains invalid header characters".into(),
    })
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

pub(crate) struct CachedToken {
    pub(crate) token: String,
    pub(crate) expires_at: Instant,
}

pub(crate) type TokenCache = Arc<RwLock<HashMap<String, CachedToken>>>;

pub(crate) async fn fetch_download_response_internal(
    client: &reqwest::Client,
    token_cache: &TokenCache,
    url: &str,
) -> Result<reqwest::Response, Error> {
    let cached_token = get_cached_token_for_url_internal(token_cache, url).await;

    let mut request = client.get(url);
    if let Some(token) = &cached_token {
        request = request.header(AUTHORIZATION, bearer_header(token)?);
    }

    let response = request.send().await.map_err(|e| Error::NetworkFailure {
        message: e.to_string(),
    })?;

    let response = if response.status() == StatusCode::UNAUTHORIZED {
        handle_auth_challenge_internal(client, token_cache, url, response).await?
    } else {
        response
    };

    if !response.status().is_success() {
        return Err(Error::NetworkFailure {
            message: format!("HTTP {}", response.status()),
        });
    }

    Ok(response)
}

pub(crate) async fn fetch_range_response_internal(
    client: &reqwest::Client,
    token_cache: &TokenCache,
    url: &str,
    range: &str,
) -> Result<reqwest::Response, Error> {
    let mut last_error = None;

    for attempt in 0..=MAX_CHUNK_RETRIES {
        let cached_token = get_cached_token_for_url_internal(token_cache, url).await;

        let mut request = client.get(url).header("Range", range);
        if let Some(token) = &cached_token {
            request = request.header(AUTHORIZATION, bearer_header(token)?);
        }

        match request.send().await {
            Ok(response) => {
                let response = if response.status() == StatusCode::UNAUTHORIZED {
                    match handle_auth_challenge_internal(client, token_cache, url, response).await {
                        Ok(resp) => resp,
                        Err(e) => return Err(e),
                    }
                } else {
                    response
                };

                if !response.status().is_success() {
                    let err = Error::NetworkFailure {
                        message: format!("HTTP {}", response.status()),
                    };

                    if response.status().is_server_error() && attempt < MAX_CHUNK_RETRIES {
                        last_error = Some(err);
                        tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                        continue;
                    }
                    return Err(err);
                }

                return Ok(response);
            }
            Err(e) => {
                last_error = Some(Error::NetworkFailure {
                    message: e.to_string(),
                });

                if attempt < MAX_CHUNK_RETRIES {
                    tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                    continue;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| Error::NetworkFailure {
        message: "range request failed after retries".into(),
    }))
}

pub(crate) async fn get_cached_token_for_url_internal(
    token_cache: &TokenCache,
    url: &str,
) -> Option<String> {
    let scope = extract_scope_for_url(url)?;
    let cache = token_cache.read().await;
    let now = Instant::now();

    cache
        .get(&scope)
        .filter(|cached| cached.expires_at > now)
        .map(|cached| cached.token.clone())
}

pub(crate) async fn handle_auth_challenge_internal(
    client: &reqwest::Client,
    token_cache: &TokenCache,
    url: &str,
    response: reqwest::Response,
) -> Result<reqwest::Response, Error> {
    let www_auth_header = response.headers().get(WWW_AUTHENTICATE);

    let www_auth = match www_auth_header {
        Some(value) => value.to_str().map_err(|_| Error::NetworkFailure {
            message: "WWW-Authenticate header contains invalid characters".to_string(),
        })?,
        None => {
            return Err(Error::NetworkFailure {
                message:
                    "server returned 401 without WWW-Authenticate header (may be rate limited)"
                        .to_string(),
            });
        }
    };

    let token = fetch_bearer_token_internal(client, token_cache, www_auth).await?;

    let response = client
        .get(url)
        .header(AUTHORIZATION, bearer_header(&token)?)
        .send()
        .await
        .map_err(|e| Error::NetworkFailure {
            message: e.to_string(),
        })?;

    if response.status() == StatusCode::UNAUTHORIZED {
        return Err(Error::NetworkFailure {
            message: "authentication failed: token was rejected by server".to_string(),
        });
    }

    Ok(response)
}

pub(crate) async fn fetch_bearer_token_internal(
    client: &reqwest::Client,
    token_cache: &TokenCache,
    www_authenticate: &str,
) -> Result<String, Error> {
    let (realm, service, scope) = parse_www_authenticate(www_authenticate)?;

    {
        let cache = token_cache.read().await;
        if let Some(cached) = cache.get(&scope)
            && cached.expires_at > Instant::now()
        {
            return Ok(cached.token.clone());
        }
    }

    let token_url =
        reqwest::Url::parse_with_params(&realm, &[("service", &service), ("scope", &scope)])
            .map_err(Error::network("failed to construct token URL"))?;

    let response = client
        .get(token_url)
        .send()
        .await
        .map_err(Error::network("token request failed"))?;

    if !response.status().is_success() {
        return Err(Error::NetworkFailure {
            message: format!("token request returned HTTP {}", response.status()),
        });
    }

    let token_response: TokenResponse = response
        .json()
        .await
        .map_err(Error::network("failed to parse token response"))?;

    {
        let mut cache = token_cache.write().await;
        cache.insert(
            scope,
            CachedToken {
                token: token_response.token.clone(),
                expires_at: Instant::now() + Duration::from_secs(240),
            },
        );
    }

    Ok(token_response.token)
}

pub(crate) fn extract_scope_for_url(url: &str) -> Option<String> {
    let marker = "ghcr.io/v2/";
    let start = url.find(marker)? + marker.len();
    let remainder = &url[start..];
    let mut parts = remainder.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    let formula = parts.next()?;
    if owner.is_empty() || repo.is_empty() || formula.is_empty() {
        return None;
    }
    Some(format!("repository:{owner}/{repo}/{formula}:pull"))
}

fn parse_www_authenticate(header: &str) -> Result<(String, String, String), Error> {
    let header = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| Error::NetworkFailure {
            message: "unsupported auth scheme".to_string(),
        })?;

    let mut realm = None;
    let mut service = None;
    let mut scope = None;

    for part in header.split(',') {
        let part = part.trim();
        if let Some((key, value)) = part.split_once('=') {
            let value = value.trim_matches('"');
            match key {
                "realm" => realm = Some(value.to_string()),
                "service" => service = Some(value.to_string()),
                "scope" => scope = Some(value.to_string()),
                _ => {}
            }
        }
    }

    let realm = realm.ok_or_else(|| Error::NetworkFailure {
        message: "missing realm in WWW-Authenticate".to_string(),
    })?;
    let service = service.ok_or_else(|| Error::NetworkFailure {
        message: "missing service in WWW-Authenticate".to_string(),
    })?;
    let scope = scope.ok_or_else(|| Error::NetworkFailure {
        message: "missing scope in WWW-Authenticate".to_string(),
    })?;

    Ok((realm, service, scope))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_scope_for_url_supports_core_packages() {
        let scope =
            extract_scope_for_url("https://ghcr.io/v2/homebrew/core/lz4/blobs/sha256:abc").unwrap();
        assert_eq!(scope, "repository:homebrew/core/lz4:pull");
    }

    #[test]
    fn extract_scope_for_url_supports_tapped_packages() {
        let scope =
            extract_scope_for_url("https://ghcr.io/v2/hashicorp/tap/terraform/blobs/sha256:abc")
                .unwrap();
        assert_eq!(scope, "repository:hashicorp/tap/terraform:pull");
    }
}
