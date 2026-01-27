use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use futures_util::future::select_all;
use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, HeaderValue, WWW_AUTHENTICATE};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock, Semaphore, mpsc};

use crate::blob::BlobCache;
use crate::progress::InstallProgress;
use zb_core::Error;

/// Number of parallel connections to race when downloading (hits different CDN edges)
const RACING_CONNECTIONS: usize = 4;

/// Delay between starting each racing connection (ms)
const RACING_STAGGER_MS: u64 = 200;

/// Callback for download progress updates
pub type DownloadProgressCallback = Arc<dyn Fn(InstallProgress) + Send + Sync>;

/// Get alternate URLs for a given primary URL (from user-configured mirrors)
fn get_alternate_urls(primary_url: &str) -> Vec<String> {
    let mut alternates = Vec::new();

    // Check for user-configured mirrors via environment variable (comma-separated)
    if let Ok(mirrors) = std::env::var("HOMEBREW_BOTTLE_MIRRORS") {
        for mirror in mirrors.split(',') {
            let mirror = mirror.trim();
            if !mirror.is_empty()
                && let Some(alt) = transform_url_to_mirror(primary_url, mirror)
            {
                alternates.push(alt);
            }
        }
    }

    alternates
}

/// Transform a URL to use a custom mirror domain
fn transform_url_to_mirror(url: &str, mirror_domain: &str) -> Option<String> {
    if url.contains("ghcr.io") {
        Some(url.replace("ghcr.io", mirror_domain))
    } else {
        None
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

/// Result of a completed download, sent via channel for streaming processing
#[derive(Debug, Clone)]
pub struct DownloadResult {
    pub name: String,
    pub sha256: String,
    pub blob_path: PathBuf,
    pub index: usize,
}

/// Cached auth token with expiry
struct CachedToken {
    token: String,
    expires_at: Instant,
}

/// Token cache keyed by scope (e.g., "repository:homebrew/core/lz4:pull")
type TokenCache = Arc<RwLock<HashMap<String, CachedToken>>>;

pub struct Downloader {
    client: reqwest::Client,
    blob_cache: BlobCache,
    token_cache: TokenCache,
}

impl Downloader {
    pub fn new(blob_cache: BlobCache) -> Self {
        // Use HTTP/2 with connection pooling for better performance
        // Note: don't use http2_prior_knowledge() as some servers (like ghcr.io) need ALPN negotiation
        Self {
            client: reqwest::Client::builder()
                .user_agent("zerobrew/0.1")
                .pool_max_idle_per_host(10)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            blob_cache,
            token_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Remove a blob from the cache (used when extraction fails due to corruption)
    pub fn remove_blob(&self, sha256: &str) -> bool {
        self.blob_cache.remove_blob(sha256).unwrap_or(false)
    }

    pub async fn download(&self, url: &str, expected_sha256: &str) -> Result<PathBuf, Error> {
        self.download_with_progress(url, expected_sha256, None, None)
            .await
    }

    pub async fn download_with_progress(
        &self,
        url: &str,
        expected_sha256: &str,
        name: Option<String>,
        progress: Option<DownloadProgressCallback>,
    ) -> Result<PathBuf, Error> {
        if self.blob_cache.has_blob(expected_sha256) {
            // Report as already complete
            if let (Some(cb), Some(n)) = (&progress, &name) {
                cb(InstallProgress::DownloadCompleted {
                    name: n.clone(),
                    total_bytes: 0,
                });
            }
            return Ok(self.blob_cache.blob_path(expected_sha256));
        }

        // Get alternate mirror URLs (user-configured)
        let alternates = get_alternate_urls(url);

        // Always use racing to hit different CDN edges for faster downloads
        self.download_with_racing(url, &alternates, expected_sha256, name, progress)
            .await
    }

    /// Download with racing: start multiple parallel connections to the same URL
    /// (hits different CDN edges) and optionally alternate mirrors.
    /// First successful download wins, others are cancelled.
    async fn download_with_racing(
        &self,
        primary_url: &str,
        alternate_urls: &[String],
        expected_sha256: &str,
        name: Option<String>,
        progress: Option<DownloadProgressCallback>,
    ) -> Result<PathBuf, Error> {
        use tokio::sync::oneshot;

        // Create a channel to signal completion
        let (done_tx, _done_rx) = oneshot::channel::<()>();
        let done_tx = Arc::new(Mutex::new(Some(done_tx)));

        // Build list of URLs to race:
        // - Multiple connections to primary URL (hits different CDN edges)
        // - Plus any configured alternate mirrors
        let mut all_urls: Vec<String> = Vec::new();

        // Add primary URL multiple times for CDN edge racing
        for _ in 0..RACING_CONNECTIONS {
            all_urls.push(primary_url.to_string());
        }

        // Add alternate mirrors
        all_urls.extend(alternate_urls.iter().cloned());

        // Spawn racing downloads
        let mut handles = Vec::new();
        for (idx, url) in all_urls.into_iter().enumerate() {
            let downloader_client = self.client.clone();
            let blob_cache = self.blob_cache.clone();
            let token_cache = self.token_cache.clone();
            let expected_sha256 = expected_sha256.to_string();
            let name = name.clone();
            // Only first connection reports progress to avoid duplicate updates
            let progress = if idx == 0 { progress.clone() } else { None };
            let done_tx = done_tx.clone();

            // Stagger starts to give earlier connections a head start
            let delay = Duration::from_millis(idx as u64 * RACING_STAGGER_MS);

            let handle = tokio::spawn(async move {
                tokio::time::sleep(delay).await;

                // Check if another download already finished
                {
                    let guard = done_tx.lock().await;
                    if guard.is_none() {
                        return Err(Error::NetworkFailure {
                            message: "cancelled: another download finished first".to_string(),
                        });
                    }
                }

                let result = download_single_internal(
                    &downloader_client,
                    &blob_cache,
                    &token_cache,
                    &url,
                    &expected_sha256,
                    name,
                    progress,
                )
                .await;

                // Signal completion if successful
                if result.is_ok() {
                    let mut guard = done_tx.lock().await;
                    if let Some(tx) = guard.take() {
                        let _ = tx.send(());
                    }
                }

                result
            });

            handles.push(handle);
        }

        // Race all handles - return first success, keep trying on failures
        let mut pending = handles;
        let mut last_error = None;

        while !pending.is_empty() {
            let (result, _index, remaining) = select_all(pending).await;
            pending = remaining;

            match result {
                Ok(Ok(path)) => {
                    // Cancel remaining downloads by dropping their handles
                    drop(pending);
                    return Ok(path);
                }
                Ok(Err(e)) => last_error = Some(e),
                Err(e) => {
                    last_error = Some(Error::NetworkFailure {
                        message: format!("task join error: {e}"),
                    })
                }
            }
        }

        Err(last_error.unwrap_or_else(|| Error::NetworkFailure {
            message: "all download attempts failed".to_string(),
        }))
    }
}

/// Internal download function that can be called from racing context
async fn download_single_internal(
    client: &reqwest::Client,
    blob_cache: &BlobCache,
    token_cache: &TokenCache,
    url: &str,
    expected_sha256: &str,
    name: Option<String>,
    progress: Option<DownloadProgressCallback>,
) -> Result<PathBuf, Error> {
    // Check cache first
    if blob_cache.has_blob(expected_sha256) {
        if let (Some(cb), Some(n)) = (&progress, &name) {
            cb(InstallProgress::DownloadCompleted {
                name: n.clone(),
                total_bytes: 0,
            });
        }
        return Ok(blob_cache.blob_path(expected_sha256));
    }

    // Try with cached token first (for GHCR URLs)
    let cached_token = get_cached_token_for_url_internal(token_cache, url).await;

    let mut request = client.get(url);
    if let Some(token) = &cached_token {
        request = request.header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
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

    download_response_internal(blob_cache, response, expected_sha256, name, progress).await
}

async fn get_cached_token_for_url_internal(token_cache: &TokenCache, url: &str) -> Option<String> {
    let scope_prefix = extract_scope_prefix(url)?;
    let cache = token_cache.read().await;
    let now = Instant::now();

    for (scope, cached) in cache.iter() {
        if scope.starts_with(&scope_prefix) && cached.expires_at > now {
            return Some(cached.token.clone());
        }
    }
    None
}

async fn handle_auth_challenge_internal(
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
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        )
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

async fn fetch_bearer_token_internal(
    client: &reqwest::Client,
    token_cache: &TokenCache,
    www_authenticate: &str,
) -> Result<String, Error> {
    let (realm, service, scope) = parse_www_authenticate(www_authenticate)?;

    // Check cache first
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
            .map_err(|e| Error::NetworkFailure {
                message: format!("failed to construct token URL: {e}"),
            })?;

    // Anonymous token request (homebrew bottles are public)
    let response = client
        .get(token_url)
        .send()
        .await
        .map_err(|e| Error::NetworkFailure {
            message: format!("token request failed: {e}"),
        })?;

    if !response.status().is_success() {
        return Err(Error::NetworkFailure {
            message: format!("token request returned HTTP {}", response.status()),
        });
    }

    let token_response: TokenResponse =
        response.json().await.map_err(|e| Error::NetworkFailure {
            message: format!("failed to parse token response: {e}"),
        })?;

    // Cache the token
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

async fn download_response_internal(
    blob_cache: &BlobCache,
    response: reqwest::Response,
    expected_sha256: &str,
    name: Option<String>,
    progress: Option<DownloadProgressCallback>,
) -> Result<PathBuf, Error> {
    let total_bytes = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    if let (Some(cb), Some(n)) = (&progress, &name) {
        cb(InstallProgress::DownloadStarted {
            name: n.clone(),
            total_bytes,
        });
    }

    let mut writer =
        blob_cache
            .start_write(expected_sha256)
            .map_err(|e| Error::NetworkFailure {
                message: format!("failed to create blob writer: {e}"),
            })?;

    let mut hasher = Sha256::new();
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::NetworkFailure {
            message: format!("failed to read chunk: {e}"),
        })?;

        downloaded += chunk.len() as u64;
        hasher.update(&chunk);
        writer
            .write_all(&chunk)
            .map_err(|e| Error::NetworkFailure {
                message: format!("failed to write chunk: {e}"),
            })?;

        if let (Some(cb), Some(n)) = (&progress, &name) {
            cb(InstallProgress::DownloadProgress {
                name: n.clone(),
                downloaded,
                total_bytes,
            });
        }
    }

    let actual_hash = format!("{:x}", hasher.finalize());

    if actual_hash != expected_sha256 {
        return Err(Error::ChecksumMismatch {
            expected: expected_sha256.to_string(),
            actual: actual_hash,
        });
    }

    // Flush and sync the file to ensure all data is written
    writer.flush().map_err(|e| Error::NetworkFailure {
        message: format!("failed to flush download: {e}"),
    })?;

    if let (Some(cb), Some(n)) = (&progress, &name) {
        cb(InstallProgress::DownloadCompleted {
            name: n.clone(),
            total_bytes: downloaded,
        });
    }

    writer.commit()
}

/// Extract scope prefix from a GHCR URL for token cache matching.
/// For URL like "https://ghcr.io/v2/homebrew/core/lz4/blobs/sha256:...",
/// returns "repository:homebrew/core/" which matches scopes like "repository:homebrew/core/lz4:pull"
fn extract_scope_prefix(url: &str) -> Option<String> {
    if url.contains("ghcr.io/v2/homebrew/core/") {
        // All homebrew/core packages use the same token server, but scopes are per-package
        // We can't reuse tokens across packages, so return the full path prefix
        Some("repository:homebrew/core/".to_string())
    } else {
        None
    }
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

pub struct DownloadRequest {
    pub url: String,
    pub sha256: String,
    pub name: String,
}

type InflightMap = HashMap<String, Arc<tokio::sync::broadcast::Sender<Result<PathBuf, String>>>>;

pub struct ParallelDownloader {
    downloader: Arc<Downloader>,
    semaphore: Arc<Semaphore>,
    inflight: Arc<Mutex<InflightMap>>,
}

impl ParallelDownloader {
    pub fn new(blob_cache: BlobCache, concurrency: usize) -> Self {
        Self {
            downloader: Arc::new(Downloader::new(blob_cache)),
            semaphore: Arc::new(Semaphore::new(concurrency)),
            inflight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Remove a blob from the cache (used when extraction fails due to corruption)
    pub fn remove_blob(&self, sha256: &str) -> bool {
        self.downloader.remove_blob(sha256)
    }

    /// Download a single file (used for retries after corruption)
    pub async fn download_single(
        &self,
        request: DownloadRequest,
        progress: Option<DownloadProgressCallback>,
    ) -> Result<PathBuf, Error> {
        Self::download_with_dedup(
            self.downloader.clone(),
            self.semaphore.clone(),
            self.inflight.clone(),
            request,
            progress,
        )
        .await
    }

    pub async fn download_all(
        &self,
        requests: Vec<DownloadRequest>,
    ) -> Result<Vec<PathBuf>, Error> {
        self.download_all_with_progress(requests, None).await
    }

    pub async fn download_all_with_progress(
        &self,
        requests: Vec<DownloadRequest>,
        progress: Option<DownloadProgressCallback>,
    ) -> Result<Vec<PathBuf>, Error> {
        let handles: Vec<_> = requests
            .into_iter()
            .map(|req| {
                let downloader = self.downloader.clone();
                let semaphore = self.semaphore.clone();
                let inflight = self.inflight.clone();
                let progress = progress.clone();

                tokio::spawn(async move {
                    Self::download_with_dedup(downloader, semaphore, inflight, req, progress).await
                })
            })
            .collect();

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            let result = handle.await.map_err(|e| Error::NetworkFailure {
                message: format!("task join error: {e}"),
            })??;
            results.push(result);
        }

        Ok(results)
    }

    /// Stream downloads as they complete, allowing concurrent extraction.
    /// Returns a receiver that yields DownloadResult for each completed download.
    /// The downloads are started immediately and results are sent as soon as each completes.
    pub fn download_streaming(
        &self,
        requests: Vec<DownloadRequest>,
        progress: Option<DownloadProgressCallback>,
    ) -> mpsc::Receiver<Result<DownloadResult, Error>> {
        let (tx, rx) = mpsc::channel(requests.len().max(1));

        for (index, req) in requests.into_iter().enumerate() {
            let downloader = self.downloader.clone();
            let semaphore = self.semaphore.clone();
            let inflight = self.inflight.clone();
            let progress = progress.clone();
            let tx = tx.clone();
            let name = req.name.clone();
            let sha256 = req.sha256.clone();

            tokio::spawn(async move {
                let result =
                    Self::download_with_dedup(downloader, semaphore, inflight, req, progress).await;
                let _ = tx
                    .send(result.map(|blob_path| DownloadResult {
                        name,
                        sha256,
                        blob_path,
                        index,
                    }))
                    .await;
            });
        }

        rx
    }

    async fn download_with_dedup(
        downloader: Arc<Downloader>,
        semaphore: Arc<Semaphore>,
        inflight: Arc<Mutex<InflightMap>>,
        req: DownloadRequest,
        progress: Option<DownloadProgressCallback>,
    ) -> Result<PathBuf, Error> {
        // Check if there's already an inflight request for this sha256
        let mut receiver = {
            let mut map = inflight.lock().await;

            if let Some(sender) = map.get(&req.sha256) {
                // Subscribe to existing inflight request
                Some(sender.subscribe())
            } else {
                // Create a new broadcast channel for this request
                let (tx, _) = tokio::sync::broadcast::channel(1);
                map.insert(req.sha256.clone(), Arc::new(tx));
                None
            }
        };

        if let Some(ref mut rx) = receiver {
            // Wait for the inflight request to complete
            let result = rx.recv().await.map_err(|e| Error::NetworkFailure {
                message: format!("broadcast recv error: {e}"),
            })?;

            return result.map_err(|msg| Error::NetworkFailure { message: msg });
        }

        // We're the first request for this sha256, do the actual download
        let _permit = semaphore
            .acquire()
            .await
            .map_err(|e| Error::NetworkFailure {
                message: format!("semaphore error: {e}"),
            })?;

        let result = downloader
            .download_with_progress(&req.url, &req.sha256, Some(req.name), progress)
            .await;

        // Notify waiters and clean up
        {
            let mut map = inflight.lock().await;
            if let Some(sender) = map.remove(&req.sha256) {
                let broadcast_result = match &result {
                    Ok(path) => Ok(path.clone()),
                    Err(e) => Err(e.to_string()),
                };
                let _ = sender.send(broadcast_result);
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn valid_checksum_passes() {
        let mock_server = MockServer::start().await;
        let content = b"hello world";
        let sha256 = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

        Mock::given(method("GET"))
            .and(path("/test.tar.gz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(content.to_vec()))
            .mount(&mock_server)
            .await;

        let tmp = TempDir::new().unwrap();
        let blob_cache = BlobCache::new(tmp.path()).unwrap();
        let downloader = Downloader::new(blob_cache);

        let url = format!("{}/test.tar.gz", mock_server.uri());
        let result = downloader.download(&url, sha256).await;

        assert!(result.is_ok());
        let blob_path = result.unwrap();
        assert!(blob_path.exists());
        assert_eq!(std::fs::read(&blob_path).unwrap(), content);
    }

    #[tokio::test]
    async fn mismatch_deletes_blob_and_errors() {
        let mock_server = MockServer::start().await;
        let content = b"hello world";
        let wrong_sha256 = "0000000000000000000000000000000000000000000000000000000000000000";

        Mock::given(method("GET"))
            .and(path("/test.tar.gz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(content.to_vec()))
            .mount(&mock_server)
            .await;

        let tmp = TempDir::new().unwrap();
        let blob_cache = BlobCache::new(tmp.path()).unwrap();
        let downloader = Downloader::new(blob_cache);

        let url = format!("{}/test.tar.gz", mock_server.uri());
        let result = downloader.download(&url, wrong_sha256).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::ChecksumMismatch { .. }));

        let blob_path = tmp
            .path()
            .join("blobs")
            .join(format!("{wrong_sha256}.tar.gz"));
        assert!(!blob_path.exists());

        let tmp_path = tmp
            .path()
            .join("tmp")
            .join(format!("{wrong_sha256}.tar.gz.part"));
        assert!(!tmp_path.exists());
    }

    #[tokio::test]
    async fn skips_download_if_blob_exists() {
        let mock_server = MockServer::start().await;
        let content = b"hello world";
        let sha256 = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

        Mock::given(method("GET"))
            .and(path("/test.tar.gz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(content.to_vec()))
            .expect(0)
            .mount(&mock_server)
            .await;

        let tmp = TempDir::new().unwrap();
        let blob_cache = BlobCache::new(tmp.path()).unwrap();

        let mut writer = blob_cache.start_write(sha256).unwrap();
        writer.write_all(content).unwrap();
        writer.commit().unwrap();

        let downloader = Downloader::new(blob_cache);
        let url = format!("{}/test.tar.gz", mock_server.uri());
        let result = downloader.download(&url, sha256).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn peak_concurrent_downloads_within_limit() {
        let mock_server = MockServer::start().await;
        let concurrent_count = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));

        let content = b"test content";
        let count_clone = concurrent_count.clone();
        let max_clone = max_concurrent.clone();

        Mock::given(method("GET"))
            .respond_with(move |_: &wiremock::Request| {
                let current = count_clone.fetch_add(1, Ordering::SeqCst) + 1;
                max_clone.fetch_max(current, Ordering::SeqCst);

                // Simulate slow download
                std::thread::sleep(Duration::from_millis(50));

                count_clone.fetch_sub(1, Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_bytes(content.to_vec())
            })
            .mount(&mock_server)
            .await;

        let tmp = TempDir::new().unwrap();
        let blob_cache = BlobCache::new(tmp.path()).unwrap();
        let downloader = ParallelDownloader::new(blob_cache, 2); // Limit to 2 concurrent

        // Create 5 different download requests
        let requests: Vec<_> = (0..5)
            .map(|i| {
                let sha256 = format!("{:064x}", i);
                DownloadRequest {
                    url: format!("{}/file{i}.tar.gz", mock_server.uri()),
                    sha256,
                    name: format!("pkg{i}"),
                }
            })
            .collect();

        let _ = downloader.download_all(requests).await;

        let peak = max_concurrent.load(Ordering::SeqCst);
        assert!(
            peak <= 2,
            "peak concurrent downloads was {peak}, expected <= 2"
        );
    }

    #[tokio::test]
    async fn same_blob_requested_multiple_times_fetches_once() {
        let mock_server = MockServer::start().await;
        let content = b"deduplicated content";

        // Compute the actual SHA256 for the content
        let actual_sha256 = {
            let mut hasher = Sha256::new();
            hasher.update(content);
            format!("{:x}", hasher.finalize())
        };

        Mock::given(method("GET"))
            .and(path("/dedup.tar.gz"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(content.to_vec())
                    .set_delay(Duration::from_millis(100)),
            )
            .expect(1) // Should only be called once
            .mount(&mock_server)
            .await;

        let tmp = TempDir::new().unwrap();
        let blob_cache = BlobCache::new(tmp.path()).unwrap();
        let downloader = ParallelDownloader::new(blob_cache, 4);

        // Create 5 requests for the SAME blob
        let requests: Vec<_> = (0..5)
            .map(|i| DownloadRequest {
                url: format!("{}/dedup.tar.gz", mock_server.uri()),
                sha256: actual_sha256.clone(),
                name: format!("dedup{i}"),
            })
            .collect();

        let results = downloader.download_all(requests).await.unwrap();

        assert_eq!(results.len(), 5);
        for path in &results {
            assert!(path.exists());
        }
        // Mock expectation of 1 call will verify deduplication worked
    }
}
