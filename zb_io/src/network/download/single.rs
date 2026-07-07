use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::future::select_all;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH};
use sha2::{Digest, Sha256};
use tokio::sync::{Notify, RwLock, Semaphore};
use tracing::warn;

use crate::network::mirror::rewrite_bottle_url;
use crate::network::tls::shared_tls_config;
use crate::progress::InstallProgress;
use crate::storage::blob::BlobCache;
use zb_core::Error;

use super::auth::{
    TokenCache, bearer_header, fetch_download_response_internal, get_cached_token_for_url_internal,
};
use super::chunked::{ChunkedDownloadContext, download_with_chunks, server_supports_ranges};
use super::{
    CHUNKED_DOWNLOAD_THRESHOLD, DownloadProgressCallback, GLOBAL_DOWNLOAD_CONCURRENCY,
    RACING_CONNECTIONS, RACING_STAGGER_MS,
};

fn get_download_url_candidates(primary_url: &str) -> (String, Vec<String>) {
    let bottle_domain = std::env::var("HOMEBREW_BOTTLE_DOMAIN")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let bottle_mirrors = std::env::var("HOMEBREW_BOTTLE_MIRRORS")
        .ok()
        .filter(|value| !value.trim().is_empty());

    get_download_url_candidates_from_env(
        primary_url,
        bottle_domain.as_deref(),
        bottle_mirrors.as_deref(),
    )
}

fn get_download_url_candidates_from_env(
    primary_url: &str,
    bottle_domain: Option<&str>,
    bottle_mirrors: Option<&str>,
) -> (String, Vec<String>) {
    let original = primary_url.to_string();
    let mut primary = original.clone();
    let mut alternates = Vec::new();

    if let Some(domain) = bottle_domain
        && let Some(mirrored) = rewrite_bottle_url(primary_url, domain)
    {
        primary = mirrored;
        if primary != original {
            alternates.push(original.clone());
        }
    }

    if let Some(mirrors) = bottle_mirrors {
        for mirror in mirrors.split(',') {
            let mirror = mirror.trim();
            if mirror.is_empty() {
                continue;
            }

            if let Some(alt) = rewrite_bottle_url(primary_url, mirror)
                && alt != primary
                && alt != original
            {
                alternates.push(alt);
            }
        }
    }

    (primary, alternates)
}

pub struct Downloader {
    client: reqwest::Client,
    pub(crate) blob_cache: BlobCache,
    pub(crate) token_cache: TokenCache,
    pub(crate) global_semaphore: Option<Arc<Semaphore>>,
    tls_config: Arc<rustls::ClientConfig>,
}

impl Downloader {
    pub fn new(blob_cache: BlobCache) -> Self {
        Self::with_semaphore(blob_cache, None)
    }

    pub fn with_semaphore(blob_cache: BlobCache, semaphore: Option<Arc<Semaphore>>) -> Self {
        let tls_config = shared_tls_config();

        let client = reqwest::Client::builder()
            .user_agent("zerobrew/0.1")
            .use_preconfigured_tls((*tls_config).clone())
            .pool_max_idle_per_host(10)
            .tcp_nodelay(true)
            .tcp_keepalive(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(300))
            .http2_adaptive_window(true)
            .http2_initial_stream_window_size(Some(2 * 1024 * 1024))
            .http2_initial_connection_window_size(Some(4 * 1024 * 1024))
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            blob_cache,
            token_cache: Arc::new(RwLock::new(HashMap::new())),
            global_semaphore: semaphore,
            tls_config,
        }
    }

    fn create_isolated_client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .user_agent("zerobrew/0.1")
            .use_preconfigured_tls((*self.tls_config).clone())
            .pool_max_idle_per_host(0)
            .tcp_nodelay(true)
            .tcp_keepalive(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(300))
            .http2_adaptive_window(true)
            .http2_initial_stream_window_size(Some(2 * 1024 * 1024))
            .http2_initial_connection_window_size(Some(4 * 1024 * 1024))
            .build()
            .expect("failed to build isolated HTTP client")
    }

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
            if let (Some(cb), Some(n)) = (&progress, &name) {
                cb(InstallProgress::DownloadCompleted {
                    name: n.clone(),
                    total_bytes: 0,
                });
            }
            return Ok(self.blob_cache.blob_path(expected_sha256));
        }

        let (primary_url, alternates) = get_download_url_candidates(url);

        self.download_with_racing(&primary_url, &alternates, expected_sha256, name, progress)
            .await
    }

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
                request = request.header(AUTHORIZATION, bearer_header(token)?);
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

            warn!(
                error = %last_error
                    .as_ref()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "unknown error".to_string()),
                "chunked download failed; falling back to single-connection download"
            );
        }

        let done = Arc::new(AtomicBool::new(false));
        let done_notify = Arc::new(Notify::new());
        let body_download_gate = Arc::new(Semaphore::new(1));

        let mut all_urls: Vec<String> = Vec::new();

        for _ in 0..RACING_CONNECTIONS {
            all_urls.push(primary_url.to_string());
        }

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
                Err(e) => last_error = Some(Error::network("task join error")(e)),
            }
        }

        Err(last_error.unwrap_or_else(|| Error::NetworkFailure {
            message: "all download attempts failed".to_string(),
        }))
    }
}

pub(crate) async fn download_response_internal(
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

    let mut writer = blob_cache
        .start_write(expected_sha256)
        .map_err(Error::network("failed to create blob writer"))?;

    let mut hasher = Sha256::new();
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(Error::network("failed to read chunk"))?;

        downloaded += chunk.len() as u64;
        hasher.update(&chunk);
        writer
            .write_all(&chunk)
            .map_err(Error::network("failed to write chunk"))?;

        if let (Some(cb), Some(n)) = (&progress, &name) {
            cb(InstallProgress::DownloadProgress {
                name: n.clone(),
                downloaded,
                total_bytes,
            });
        }
    }

    let actual_hash = crate::checksum::sha256_hex(hasher);

    if actual_hash != expected_sha256 {
        return Err(Error::ChecksumMismatch {
            expected: expected_sha256.to_string(),
            actual: actual_hash,
        });
    }

    writer
        .flush()
        .map_err(Error::network("failed to flush download"))?;

    if let (Some(cb), Some(n)) = (&progress, &name) {
        cb(InstallProgress::DownloadCompleted {
            name: n.clone(),
            total_bytes: downloaded,
        });
    }

    writer.commit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn prefers_homebrew_bottle_domain_before_original_url() {
        let (primary, alternates) = get_download_url_candidates_from_env(
            "https://ghcr.io/v2/homebrew/core/test/blobs/sha256:abc",
            Some("https://mirrors.ustc.edu.cn/homebrew-bottles"),
            None,
        );

        assert_eq!(
            primary,
            "https://mirrors.ustc.edu.cn/homebrew-bottles/v2/homebrew/core/test/blobs/sha256:abc"
        );
        assert_eq!(
            alternates,
            vec!["https://ghcr.io/v2/homebrew/core/test/blobs/sha256:abc".to_string()]
        );
    }

    #[test]
    fn keeps_original_url_when_bottle_domain_is_unusable() {
        let (primary, alternates) = get_download_url_candidates_from_env(
            "https://example.com/file.tar.gz",
            Some("not a url"),
            None,
        );

        assert_eq!(primary, "https://example.com/file.tar.gz");
        assert!(alternates.is_empty());
    }

    #[test]
    fn appends_legacy_bottle_mirrors_after_primary_and_original() {
        let (primary, alternates) = get_download_url_candidates_from_env(
            "https://ghcr.io/v2/homebrew/core/test/blobs/sha256:abc",
            Some("https://mirrors.ustc.edu.cn/homebrew-bottles"),
            Some("https://mirror.example.com, https://backup.example.com"),
        );

        assert_eq!(
            primary,
            "https://mirrors.ustc.edu.cn/homebrew-bottles/v2/homebrew/core/test/blobs/sha256:abc"
        );
        assert_eq!(
            alternates,
            vec![
                "https://ghcr.io/v2/homebrew/core/test/blobs/sha256:abc".to_string(),
                "https://mirror.example.com/v2/homebrew/core/test/blobs/sha256:abc".to_string(),
                "https://backup.example.com/v2/homebrew/core/test/blobs/sha256:abc".to_string()
            ]
        );
    }

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
}
