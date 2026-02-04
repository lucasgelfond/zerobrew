use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use futures_util::future::select_all;
use reqwest::StatusCode;
use reqwest::header::{
    ACCEPT_RANGES, AUTHORIZATION, CONTENT_LENGTH, CONTENT_RANGE, HeaderValue, WWW_AUTHENTICATE,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Notify, RwLock, Semaphore, mpsc};

use crate::blob::BlobCache;
use crate::progress::InstallProgress;
use zb_core::Error;

const RACING_CONNECTIONS: usize = 3;
const RACING_STAGGER_MS: u64 = 200;

/// Minimum file size to use chunked downloads (10MB)
const CHUNKED_DOWNLOAD_THRESHOLD: u64 = 10 * 1024 * 1024;

/// Global download concurrency limit
/// Total number of concurrent connections across all downloads to avoid
/// overwhelming servers and the local network. Based on industry best practices
/// (npm uses 20-50, we use a conservative 20 for HTTP/1.1 compatibility).
const GLOBAL_DOWNLOAD_CONCURRENCY: usize = 20;

/// Maximum concurrent chunk downloads per file
/// Chosen to divide GLOBAL_DOWNLOAD_CONCURRENCY among multiple large file downloads.
/// With 20 global concurrency, we can have 3-4 large files downloading concurrently.
const MAX_CONCURRENT_CHUNKS: usize = 6;

/// Maximum retry attempts for failed chunk downloads
const MAX_CHUNK_RETRIES: u32 = 3;

fn calculate_chunk_size(file_size: u64) -> u64 {
    const MIN_CHUNK_SIZE: u64 = 5 * 1024 * 1024;
    const MAX_CHUNK_SIZE: u64 = 20 * 1024 * 1024;

    let target_chunks = MAX_CONCURRENT_CHUNKS as u64;
    let chunk_size = file_size / target_chunks;

    chunk_size.clamp(MIN_CHUNK_SIZE, MAX_CHUNK_SIZE)
}

/// Context for chunk download operations
struct ChunkDownloadContext<'a> {
    client: &'a reqwest::Client,
    token_cache: &'a TokenCache,
    url: &'a str,
    progress: Option<DownloadProgressCallback>,
    name: Option<String>,
    file_size: u64,
    total_downloaded: Arc<AtomicU64>,
}

/// Context for chunked download operations
struct ChunkedDownloadContext<'a> {
    blob_cache: &'a BlobCache,
    client: &'a reqwest::Client,
    token_cache: &'a TokenCache,
    url: &'a str,
    expected_sha256: &'a str,
    name: Option<String>,
    progress: Option<DownloadProgressCallback>,
    file_size: u64,
    global_semaphore: &'a Arc<Semaphore>,
}
// FIXME: extract timeout and HTTP/2 window size constants to config file

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

type TokenCache = Arc<RwLock<HashMap<String, CachedToken>>>;

fn build_rustls_config() -> rustls::ClientConfig {
    let provider = rustls::crypto::aws_lc_rs::default_provider();

    let mut root_store = rustls::RootCertStore::empty();

    for cert in rustls_native_certs::load_native_certs().expect("failed to load native certs") {
        root_store.add(cert).ok();
    }

    rustls::ClientConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .expect("failed to set protocol versions")
        .with_root_certificates(root_store)
        .with_no_client_auth()
}

pub struct Downloader {
    client: reqwest::Client,
    blob_cache: BlobCache,
    token_cache: TokenCache,
    global_semaphore: Option<Arc<Semaphore>>,
    tls_config: Arc<rustls::ClientConfig>,
}

impl Downloader {
    pub fn new(blob_cache: BlobCache) -> Self {
        Self::with_semaphore(blob_cache, None)
    }

    pub fn with_semaphore(blob_cache: BlobCache, semaphore: Option<Arc<Semaphore>>) -> Self {
        // Use HTTP/2 with connection pooling for better performance
        let tls_config = Arc::new(build_rustls_config());

        Self {
            client: reqwest::Client::builder()
                .user_agent("zerobrew/0.1")
                .pool_max_idle_per_host(10)
                .tcp_nodelay(true)
                .tcp_keepalive(Duration::from_secs(60))
                .connect_timeout(Duration::from_secs(30))
                .timeout(Duration::from_secs(300))
                .http2_adaptive_window(true)
                .http2_initial_stream_window_size(Some(2 * 1024 * 1024))
                .http2_initial_connection_window_size(Some(4 * 1024 * 1024))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            blob_cache,
            token_cache: Arc::new(RwLock::new(HashMap::new())),
            global_semaphore: semaphore,
            tls_config,
        }
    }

    // FIXME: extract timeout and HTTP/2 window size constants to config file
    fn create_isolated_client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .user_agent("zerobrew/0.1")
            .use_preconfigured_tls(self.tls_config.clone())
            .pool_max_idle_per_host(0)
            .tcp_nodelay(true)
            .tcp_keepalive(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(300))
            .http2_adaptive_window(true)
            .http2_initial_stream_window_size(Some(2 * 1024 * 1024))
            .http2_initial_connection_window_size(Some(4 * 1024 * 1024))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
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
        let (use_chunked, file_size) = {
            let cached_token =
                get_cached_token_for_url_internal(&self.token_cache, primary_url).await;

            let mut request = self.client.head(primary_url);
            if let Some(token) = &cached_token {
                request = request.header(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
                );
            }

            match request.send().await {
                Ok(response) if response.status().is_success() => {
                    let content_length = response
                        .headers()
                        .get(CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok());

                    let supports_ranges = server_supports_ranges(&response);

                    if let Some(size) = content_length {
                        (
                            supports_ranges && size >= CHUNKED_DOWNLOAD_THRESHOLD,
                            Some(size),
                        )
                    } else {
                        (false, None)
                    }
                }
                _ => (false, None),
            }
        };

        if use_chunked && let Some(size) = file_size {
            // Use global semaphore if available, otherwise create a temporary one
            let semaphore = self
                .global_semaphore
                .clone()
                .unwrap_or_else(|| Arc::new(Semaphore::new(GLOBAL_DOWNLOAD_CONCURRENCY)));

            let mut all_urls = Vec::new();
            all_urls.push(primary_url.to_string());
            all_urls.extend(alternate_urls.iter().cloned());

            let mut last_error = None;
            for url in &all_urls {
                let ctx = ChunkedDownloadContext {
                    blob_cache: &self.blob_cache,
                    client: &self.client,
                    token_cache: &self.token_cache,
                    url: url.as_str(),
                    expected_sha256,
                    name: name.clone(),
                    progress: progress.clone(),
                    file_size: size,
                    global_semaphore: &semaphore,
                };

                match download_with_chunks(&ctx).await {
                    Ok(path) => return Ok(path),
                    Err(err) => last_error = Some(err),
                }
            }

            return Err(last_error.unwrap_or_else(|| Error::NetworkFailure {
                message: "all chunked download attempts failed".to_string(),
            }));
        }

        // Otherwise, use the existing racing logic
        let done = Arc::new(AtomicBool::new(false));
        let done_notify = Arc::new(Notify::new());
        let body_download_gate = Arc::new(Semaphore::new(1));

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

        let mut handles = Vec::new();
        for (idx, url) in all_urls.into_iter().enumerate() {
            let downloader_client = if idx < RACING_CONNECTIONS {
                self.create_isolated_client()
            } else {
                self.client.clone()
            };
            let blob_cache = self.blob_cache.clone();
            let token_cache = self.token_cache.clone();
            let expected_sha256 = expected_sha256.to_string();
            let name = name.clone();
            let progress = progress.clone();
            let done = done.clone();
            let done_notify = done_notify.clone();
            let body_download_gate = body_download_gate.clone();

            let delay = Duration::from_millis(idx as u64 * RACING_STAGGER_MS);

            let handle = tokio::spawn(async move {
                tokio::time::sleep(delay).await;

                if done.load(Ordering::Acquire) {
                    return Err(Error::NetworkFailure {
                        message: "cancelled: another download finished first".to_string(),
                    });
                }

                // Another racing task may have already created the final blob.
                if blob_cache.has_blob(&expected_sha256) {
                    if let (Some(cb), Some(n)) = (&progress, &name) {
                        cb(InstallProgress::DownloadCompleted {
                            name: n.clone(),
                            total_bytes: 0,
                        });
                    }

                    done.store(true, Ordering::Release);
                    done_notify.notify_waiters();
                    return Ok(blob_cache.blob_path(&expected_sha256));
                }

                let response =
                    fetch_download_response_internal(&downloader_client, &token_cache, &url)
                        .await?;

                let _permit = tokio::select! {
                    permit = body_download_gate.acquire_owned() => permit.map_err(|_| Error::NetworkFailure {
                        message: "download permit closed unexpectedly".to_string(),
                    })?,
                    _ = done_notify.notified() => {
                        return Err(Error::NetworkFailure {
                            message: "cancelled: another download finished first".to_string(),
                        });
                    }
                };

                if done.load(Ordering::Acquire) {
                    return Err(Error::NetworkFailure {
                        message: "cancelled: another download finished first".to_string(),
                    });
                }

                // Another racing task may have created the blob while we waited for the permit.
                if blob_cache.has_blob(&expected_sha256) {
                    if let (Some(cb), Some(n)) = (&progress, &name) {
                        cb(InstallProgress::DownloadCompleted {
                            name: n.clone(),
                            total_bytes: 0,
                        });
                    }

                    done.store(true, Ordering::Release);
                    done_notify.notify_waiters();
                    return Ok(blob_cache.blob_path(&expected_sha256));
                }

                let result = download_response_internal(
                    &blob_cache,
                    response,
                    &expected_sha256,
                    name,
                    progress,
                )
                .await;

                if result.is_ok() {
                    done.store(true, Ordering::Release);
                    done_notify.notify_waiters();
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
                    for handle in &pending {
                        handle.abort();
                    }
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

/// Fetch a successful download response with GHCR auth handling.
async fn fetch_download_response_internal(
    client: &reqwest::Client,
    token_cache: &TokenCache,
    url: &str,
) -> Result<reqwest::Response, Error> {
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

    Ok(response)
}

async fn fetch_range_response_internal(
    client: &reqwest::Client,
    token_cache: &TokenCache,
    url: &str,
    range: &str,
) -> Result<reqwest::Response, Error> {
    let cached_token = get_cached_token_for_url_internal(token_cache, url).await;

    let mut request = client.get(url).header("Range", range);
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

    Ok(response)
}

async fn get_cached_token_for_url_internal(token_cache: &TokenCache, url: &str) -> Option<String> {
    let scope = extract_scope_for_url(url)?;
    let cache = token_cache.read().await;
    let now = Instant::now();

    cache
        .get(&scope)
        .filter(|cached| cached.expires_at > now)
        .map(|cached| cached.token.clone())
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

struct ChunkRange {
    offset: u64,
    size: u64,
}

fn server_supports_ranges(response: &reqwest::Response) -> bool {
    response
        .headers()
        .get(ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "bytes")
        .unwrap_or(false)
}

fn calculate_chunk_ranges(file_size: u64) -> Vec<ChunkRange> {
    let chunk_size = calculate_chunk_size(file_size);
    let mut chunks = Vec::new();
    let mut offset = 0;

    while offset < file_size {
        let remaining = file_size - offset;
        let chunk_size = remaining.min(chunk_size);
        chunks.push(ChunkRange {
            offset,
            size: chunk_size,
        });
        offset += chunk_size;
    }

    chunks
}

async fn download_chunk(
    ctx: &ChunkDownloadContext<'_>,
    chunk: &ChunkRange,
) -> Result<Vec<u8>, Error> {
    let range_header = format!("bytes={}-{}", chunk.offset, chunk.offset + chunk.size - 1);

    let mut last_error = None;

    for attempt in 0..=MAX_CHUNK_RETRIES {
        let cached_token = get_cached_token_for_url_internal(ctx.token_cache, ctx.url).await;

        let mut request = ctx
            .client
            .get(ctx.url)
            .header("Range", range_header.clone());
        if let Some(token) = &cached_token {
            request = request.header(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
            );
        }

        match request.send().await {
            Ok(response) => {
                if response.status() == StatusCode::UNAUTHORIZED {
                    let www_auth = match response.headers().get(WWW_AUTHENTICATE) {
                        Some(value) => value.to_str().map_err(|_| Error::NetworkFailure {
                            message: "WWW-Authenticate header contains invalid characters"
                                .to_string(),
                        })?,
                        None => {
                            return Err(Error::NetworkFailure {
                                message: "server returned 401 without WWW-Authenticate header"
                                    .to_string(),
                            });
                        }
                    };

                    match fetch_bearer_token_internal(ctx.client, ctx.token_cache, www_auth).await {
                        Ok(_new_token) => {
                            last_error = Some(Error::NetworkFailure {
                                message: "token expired, retrying with new token".to_string(),
                            });
                            continue;
                        }
                        Err(e) => {
                            return Err(Error::NetworkFailure {
                                message: format!("failed to refresh token: {e}"),
                            });
                        }
                    }
                }

                if let Some(content_range) = response.headers().get(CONTENT_RANGE) {
                    let range_str = content_range.to_str().unwrap_or("");
                    if !range_str.contains(&format!(
                        "{}-{}",
                        chunk.offset,
                        chunk.offset + chunk.size - 1
                    )) {
                        return Err(Error::NetworkFailure {
                            message: format!(
                                "invalid content-range: expected bytes {}-{}, got: {}",
                                chunk.offset,
                                chunk.offset + chunk.size - 1,
                                range_str
                            ),
                        });
                    }
                }

                if !response.status().is_success() {
                    last_error = Some(Error::NetworkFailure {
                        message: format!("chunk download returned HTTP {}", response.status()),
                    });

                    if response.status().is_server_error() && attempt < MAX_CHUNK_RETRIES {
                        tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                        continue;
                    }
                    return Err(last_error.unwrap());
                }

                let mut chunk_data = Vec::with_capacity(chunk.size as usize);
                let mut stream = response.bytes_stream();

                while let Some(item) = stream.next().await {
                    let bytes = item.map_err(|e| Error::NetworkFailure {
                        message: format!("failed to read chunk bytes: {e}"),
                    })?;

                    chunk_data.extend_from_slice(&bytes);

                    if let (Some(cb), Some(n)) = (&ctx.progress, &ctx.name) {
                        let downloaded = ctx
                            .total_downloaded
                            .fetch_add(bytes.len() as u64, Ordering::Release);
                        cb(InstallProgress::DownloadProgress {
                            name: n.clone(),
                            downloaded: downloaded + bytes.len() as u64,
                            total_bytes: Some(ctx.file_size),
                        });
                    }
                }

                if chunk_data.len() != chunk.size as usize {
                    return Err(Error::NetworkFailure {
                        message: format!(
                            "chunk size mismatch: expected {} bytes, got {} bytes",
                            chunk.size,
                            chunk_data.len()
                        ),
                    });
                }

                return Ok(chunk_data);
            }
            Err(e) => {
                last_error = Some(Error::NetworkFailure {
                    message: format!("chunk download failed: {e}"),
                });

                // Retry on network errors
                if attempt < MAX_CHUNK_RETRIES {
                    tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                    continue;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| Error::NetworkFailure {
        message: "chunk download failed after retries".to_string(),
    }))
}

/// Download a file using parallel chunk requests
async fn download_with_chunks(ctx: &ChunkedDownloadContext<'_>) -> Result<PathBuf, Error> {
    if !validate_range_support(ctx).await? {
        let response =
            fetch_download_response_internal(ctx.client, ctx.token_cache, ctx.url).await?;
        return download_response_internal(
            ctx.blob_cache,
            response,
            ctx.expected_sha256,
            ctx.name.clone(),
            ctx.progress.clone(),
        )
        .await;
    }

    let chunks = calculate_chunk_ranges(ctx.file_size);

    if let (Some(cb), Some(n)) = (&ctx.progress, &ctx.name) {
        cb(InstallProgress::DownloadStarted {
            name: n.clone(),
            total_bytes: Some(ctx.file_size),
        });
    }

    // Create output file early for streaming writes
    let mut writer = ctx
        .blob_cache
        .start_write(ctx.expected_sha256)
        .map_err(|e| Error::NetworkFailure {
            message: format!("failed to create blob writer: {e}"),
        })?;

    // Track expected chunk sizes for validation
    let expected_chunks: BTreeMap<u64, u64> = chunks.iter().map(|c| (c.offset, c.size)).collect();
    let total_chunks = chunks.len();

    // Channel to receive completed chunks
    let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel::<(Vec<u8>, u64)>();

    let total_downloaded = Arc::new(AtomicU64::new(0));

    // Spawn download tasks and collect handles
    let mut handles = Vec::new();
    for chunk in chunks {
        let client = ctx.client.clone();
        let token_cache = ctx.token_cache.clone();
        let url = ctx.url.to_string();
        let global_semaphore = ctx.global_semaphore.clone();
        let total_downloaded = total_downloaded.clone();
        let progress = ctx.progress.clone();
        let name = ctx.name.clone();
        let chunk_tx = chunk_tx.clone();
        let file_size = ctx.file_size;

        let handle = tokio::spawn(async move {
            // Acquire permit from global semaphore
            let _permit = global_semaphore
                .acquire()
                .await
                .map_err(|e| Error::NetworkFailure {
                    message: format!("global semaphore error: {e}"),
                })?;

            let chunk_ctx = ChunkDownloadContext {
                client: &client,
                token_cache: &token_cache,
                url: &url,
                progress: progress.clone(),
                name: name.clone(),
                file_size,
                total_downloaded: total_downloaded.clone(),
            };

            let chunk_data = download_chunk(&chunk_ctx, &chunk).await?;

            chunk_tx
                .send((chunk_data, chunk.offset))
                .map_err(|e| Error::NetworkFailure {
                    message: format!("failed to send chunk: {e}"),
                })?;

            Ok::<(), Error>(())
        });

        handles.push(handle);
    }

    // Drop our sender so the channel closes when all tasks complete
    drop(chunk_tx);

    // Track next expected offset for streaming writes
    let mut next_expected_offset: u64 = 0;
    let mut received_chunks = BTreeMap::new(); // Only buffer out-of-order chunks
    let mut chunks_written = 0u64;
    let mut hasher = Sha256::new();

    while let Some((chunk_data, offset)) = chunk_rx.recv().await {
        // Validate chunk size matches expected
        let expected_size = expected_chunks
            .get(&offset)
            .ok_or_else(|| Error::NetworkFailure {
                message: format!("received unexpected chunk at offset {}", offset),
            })?;

        if chunk_data.len() != *expected_size as usize {
            return Err(Error::NetworkFailure {
                message: format!(
                    "chunk size mismatch at offset {}: expected {} bytes, got {} bytes",
                    offset,
                    expected_size,
                    chunk_data.len()
                ),
            });
        }

        received_chunks.insert(offset, chunk_data);
        chunks_written += 1;

        while let Some((offset, _chunk_data)) = received_chunks.first_key_value() {
            if *offset != next_expected_offset {
                break;
            }

            let (_, chunk_data) = received_chunks.pop_first().unwrap();
            hasher.update(&chunk_data);
            writer
                .write_all(&chunk_data)
                .map_err(|e| Error::NetworkFailure {
                    message: format!(
                        "failed to write chunk at offset {}: {e}",
                        next_expected_offset
                    ),
                })?;

            next_expected_offset += chunk_data.len() as u64;
        }
    }

    // Wait for all download tasks to complete and check for errors
    for handle in handles {
        handle.await.map_err(|e| Error::NetworkFailure {
            message: format!("chunk download task failed: {e}"),
        })??;
    }

    if chunks_written as usize != total_chunks {
        return Err(Error::NetworkFailure {
            message: format!(
                "expected {} chunks, received {}",
                total_chunks, chunks_written
            ),
        });
    }

    if next_expected_offset != ctx.file_size {
        return Err(Error::NetworkFailure {
            message: format!(
                "incomplete write: expected {} bytes, wrote {} bytes",
                ctx.file_size, next_expected_offset
            ),
        });
    }

    let actual_hash = format!("{:x}", hasher.finalize());

    if actual_hash != ctx.expected_sha256 {
        return Err(Error::ChecksumMismatch {
            expected: ctx.expected_sha256.to_string(),
            actual: actual_hash,
        });
    }

    writer.flush().map_err(|e| Error::NetworkFailure {
        message: format!("failed to flush download: {e}"),
    })?;

    if let (Some(cb), Some(n)) = (&ctx.progress, &ctx.name) {
        cb(InstallProgress::DownloadCompleted {
            name: n.clone(),
            total_bytes: ctx.file_size,
        });
    }

    writer.commit()
}

async fn validate_range_support(ctx: &ChunkedDownloadContext<'_>) -> Result<bool, Error> {
    let response =
        fetch_range_response_internal(ctx.client, ctx.token_cache, ctx.url, "bytes=0-0").await?;

    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Ok(false);
    }

    let content_range = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    Ok(content_range.contains("0-0"))
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

/// Extract full scope from a GHCR URL for token cache matching.
/// For URL like "https://ghcr.io/v2/homebrew/core/lz4/blobs/sha256:...",
/// returns "repository:homebrew/core/lz4:pull".
fn extract_scope_for_url(url: &str) -> Option<String> {
    let marker = "ghcr.io/v2/homebrew/core/";
    let start = url.find(marker)? + marker.len();
    let remainder = &url[start..];
    let formula = remainder.split('/').next()?;
    if formula.is_empty() {
        return None;
    }
    Some(format!("repository:homebrew/core/{formula}:pull"))
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
    pub fn new(blob_cache: BlobCache) -> Self {
        let semaphore = Arc::new(Semaphore::new(GLOBAL_DOWNLOAD_CONCURRENCY));
        Self {
            downloader: Arc::new(Downloader::with_semaphore(
                blob_cache,
                Some(semaphore.clone()),
            )),
            semaphore,
            inflight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new ParallelDownloader with custom concurrency limit
    /// This allows for experimentation and tuning of the optimal concurrency level.
    pub fn with_concurrency(blob_cache: BlobCache, concurrency: usize) -> Self {
        let semaphore = Arc::new(Semaphore::new(concurrency));
        Self {
            downloader: Arc::new(Downloader::with_semaphore(
                blob_cache,
                Some(semaphore.clone()),
            )),
            semaphore,
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
        let downloader = ParallelDownloader::new(blob_cache); // Uses global concurrency

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
            peak <= GLOBAL_DOWNLOAD_CONCURRENCY,
            "peak concurrent downloads was {peak}, expected <= {GLOBAL_DOWNLOAD_CONCURRENCY}"
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
        let downloader = ParallelDownloader::new(blob_cache);

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

    #[tokio::test]
    async fn chunked_download_for_large_files() {
        let mock_server = MockServer::start().await;

        let large_content = vec![0xABu8; 15 * 1024 * 1024];
        let actual_sha256 = {
            let mut hasher = Sha256::new();
            hasher.update(&large_content);
            format!("{:x}", hasher.finalize())
        };

        Mock::given(method("HEAD"))
            .and(path("/large.tar.gz"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Accept-Ranges", "bytes")
                    .append_header("Content-Length", large_content.len().to_string()),
            )
            .mount(&mock_server)
            .await;

        let range_requests = Arc::new(AtomicUsize::new(0));
        let range_requests_clone = range_requests.clone();
        let large_content_for_closure = large_content.clone();

        Mock::given(method("GET"))
            .and(path("/large.tar.gz"))
            .respond_with(move |req: &wiremock::Request| {
                // Check if Range header is present
                if let Some(range_header) = req.headers.get("Range") {
                    range_requests_clone.fetch_add(1, Ordering::SeqCst);

                    // Parse Range header (format: "bytes=start-end")
                    let range_str = range_header.to_str().unwrap();
                    let range_part = range_str.strip_prefix("bytes=").unwrap();
                    let (start_str, end_str) = range_part.split_once('-').unwrap();
                    let start: usize = start_str.parse().unwrap();
                    let end: usize = end_str.parse().unwrap();

                    let chunk = &large_content_for_closure[start..=end];
                    ResponseTemplate::new(206) // 206 Partial Content
                        .append_header("Content-Length", chunk.len().to_string())
                        .append_header("Content-Range", format!("bytes {}-{}/{}", start, end, large_content_for_closure.len()))
                        .set_body_bytes(chunk.to_vec())
                } else {
                    // Fallback to full content
                    ResponseTemplate::new(200).set_body_bytes(large_content_for_closure.clone())
                }
            })
            .mount(&mock_server)
            .await;

        let tmp = TempDir::new().unwrap();
        let blob_cache = BlobCache::new(tmp.path()).unwrap();
        let downloader = Downloader::new(blob_cache);

        let url = format!("{}/large.tar.gz", mock_server.uri());
        let result = downloader.download(&url, &actual_sha256).await;

        assert!(result.is_ok(), "Download failed: {:?}", result.err());
        let blob_path = result.unwrap();
        assert!(blob_path.exists());

        let range_count = range_requests.load(Ordering::SeqCst);
        assert!(
            range_count > 0,
            "Expected multiple Range requests, got {}",
            range_count
        );

        let downloaded_content = std::fs::read(&blob_path).unwrap();
        assert_eq!(downloaded_content.len(), large_content.len());
        assert_eq!(downloaded_content, large_content);
    }

    #[tokio::test]
    async fn fallback_to_normal_download_when_ranges_not_supported() {
        let mock_server = MockServer::start().await;

        let large_content = vec![0xCDu8; 15 * 1024 * 1024];
        let actual_sha256 = {
            let mut hasher = Sha256::new();
            hasher.update(&large_content);
            format!("{:x}", hasher.finalize())
        };

        Mock::given(method("HEAD"))
            .and(path("/large.tar.gz"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Content-Length", large_content.len().to_string()),
            )
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/large.tar.gz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(large_content.clone()))
            .mount(&mock_server)
            .await;

        let tmp = TempDir::new().unwrap();
        let blob_cache = BlobCache::new(tmp.path()).unwrap();
        let downloader = Downloader::new(blob_cache);

        let url = format!("{}/large.tar.gz", mock_server.uri());
        let result = downloader.download(&url, &actual_sha256).await;

        assert!(result.is_ok());
        let blob_path = result.unwrap();
        assert!(blob_path.exists());

        let downloaded_content = std::fs::read(&blob_path).unwrap();
        assert_eq!(downloaded_content, large_content);
    }

    #[tokio::test]
    async fn small_files_dont_use_chunked_download() {
        let mock_server = MockServer::start().await;

        let small_content = vec![0xEFu8; 1024 * 1024];
        let actual_sha256 = {
            let mut hasher = Sha256::new();
            hasher.update(&small_content);
            format!("{:x}", hasher.finalize())
        };

        Mock::given(method("HEAD"))
            .and(path("/small.tar.gz"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Accept-Ranges", "bytes")
                    .append_header("Content-Length", small_content.len().to_string()),
            )
            .mount(&mock_server)
            .await;

        let range_used = Arc::new(AtomicUsize::new(0));
        let range_used_clone = range_used.clone();
        let small_content_for_closure = small_content.clone();

        Mock::given(method("GET"))
            .and(path("/small.tar.gz"))
            .respond_with(move |req: &wiremock::Request| {
                if req.headers.get("Range").is_some() {
                    range_used_clone.fetch_add(1, Ordering::SeqCst);
                }
                ResponseTemplate::new(200).set_body_bytes(small_content_for_closure.clone())
            })
            .mount(&mock_server)
            .await;

        let tmp = TempDir::new().unwrap();
        let blob_cache = BlobCache::new(tmp.path()).unwrap();
        let downloader = Downloader::new(blob_cache);

        let url = format!("{}/small.tar.gz", mock_server.uri());
        let result = downloader.download(&url, &actual_sha256).await;

        assert!(result.is_ok());
        let blob_path = result.unwrap();
        assert!(blob_path.exists());

        let range_count = range_used.load(Ordering::SeqCst);
        assert_eq!(
            range_count, 0,
            "Small files should not use chunked download"
        );

        let downloaded_content = std::fs::read(&blob_path).unwrap();
        assert_eq!(downloaded_content, small_content);
    }

    #[tokio::test]
    async fn chunked_download_respects_concurrency_limit() {
        let mock_server = MockServer::start().await;

        // Create a 40MB file (8 chunks of 5MB each)
        let large_content = vec![0xABu8; 40 * 1024 * 1024];
        let actual_sha256 = {
            let mut hasher = Sha256::new();
            hasher.update(&large_content);
            format!("{:x}", hasher.finalize())
        };

        // Mock HEAD request
        Mock::given(method("HEAD"))
            .and(path("/large.tar.gz"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("Accept-Ranges", "bytes")
                    .append_header("Content-Length", large_content.len().to_string()),
            )
            .mount(&mock_server)
            .await;

        // Track concurrent connections
        let concurrent_count = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));
        let concurrent_clone = concurrent_count.clone();
        let max_clone = max_concurrent.clone();
        let large_content_for_closure = large_content.clone();

        Mock::given(method("GET"))
            .and(path("/large.tar.gz"))
            .respond_with(move |req: &wiremock::Request| {
                if let Some(range_header) = req.headers.get("Range") {
                    // Track concurrent connections
                    let current = concurrent_clone.fetch_add(1, Ordering::SeqCst) + 1;
                    max_clone.fetch_max(current, Ordering::SeqCst);

                    // Parse Range header
                    let range_str = range_header.to_str().unwrap();
                    let range_part = range_str.strip_prefix("bytes=").unwrap();
                    let (start_str, end_str) = range_part.split_once('-').unwrap();
                    let start: usize = start_str.parse().unwrap();
                    let end: usize = end_str.parse().unwrap();

                    // Simulate some delay to ensure concurrent requests overlap
                    std::thread::sleep(Duration::from_millis(50));

                    let chunk = &large_content_for_closure[start..=end];

                    // Release after getting chunk
                    concurrent_clone.fetch_sub(1, Ordering::SeqCst);

                    ResponseTemplate::new(206)
                        .append_header("Content-Length", chunk.len().to_string())
                        .append_header(
                            "Content-Range",
                            format!(
                                "bytes {}-{}/{}",
                                start,
                                end,
                                large_content_for_closure.len()
                            ),
                        )
                        .set_body_bytes(chunk.to_vec())
                } else {
                    ResponseTemplate::new(200).set_body_bytes(large_content_for_closure.clone())
                }
            })
            .mount(&mock_server)
            .await;

        let tmp = TempDir::new().unwrap();
        let blob_cache = BlobCache::new(tmp.path()).unwrap();
        let downloader = Downloader::new(blob_cache);

        let url = format!("{}/large.tar.gz", mock_server.uri());
        let result = downloader.download(&url, &actual_sha256).await;

        assert!(result.is_ok(), "Download failed: {:?}", result.err());
        let blob_path = result.unwrap();
        assert!(blob_path.exists());

        // Verify that concurrency was limited
        let peak = max_concurrent.load(Ordering::SeqCst);
        assert!(
            peak <= MAX_CONCURRENT_CHUNKS,
            "Peak concurrent downloads was {peak}, expected <= {MAX_CONCURRENT_CHUNKS}"
        );

        // Verify content matches
        let downloaded_content = std::fs::read(&blob_path).unwrap();
        assert_eq!(downloaded_content.len(), large_content.len());
        assert_eq!(downloaded_content, large_content);
    }
}
