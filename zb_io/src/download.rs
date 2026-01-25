use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use futures_util::StreamExt;
use reqwest::header::{HeaderValue, AUTHORIZATION, CONTENT_LENGTH, WWW_AUTHENTICATE};
use reqwest::StatusCode;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Semaphore};

use crate::blob::BlobCache;
use crate::progress::InstallProgress;
use zb_core::Error;

/// Callback for download progress updates
pub type DownloadProgressCallback = Arc<dyn Fn(InstallProgress) + Send + Sync>;

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

pub struct Downloader {
    client: reqwest::Client,
    blob_cache: BlobCache,
}

impl Downloader {
    pub fn new(blob_cache: BlobCache) -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("zerobrew/0.1")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            blob_cache,
        }
    }

    pub async fn download(&self, url: &str, expected_sha256: &str) -> Result<PathBuf, Error> {
        self.download_with_progress(url, expected_sha256, None, None).await
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

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| Error::NetworkFailure {
                message: e.to_string(),
            })?;

        let response = if response.status() == StatusCode::UNAUTHORIZED {
            self.handle_auth_challenge(url, response).await?
        } else {
            response
        };

        if !response.status().is_success() {
            return Err(Error::NetworkFailure {
                message: format!("HTTP {}", response.status()),
            });
        }

        self.download_response_with_progress(response, expected_sha256, name, progress).await
    }

    async fn handle_auth_challenge(
        &self,
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
                    message: "server returned 401 without WWW-Authenticate header (may be rate limited)".to_string(),
                });
            }
        };

        let token = self.fetch_bearer_token(www_auth).await?;

        let response = self
            .client
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

        // If we still get 401 after providing a token, give a clearer error
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(Error::NetworkFailure {
                message: "authentication failed: token was rejected by server".to_string(),
            });
        }

        Ok(response)
    }

    async fn fetch_bearer_token(&self, www_authenticate: &str) -> Result<String, Error> {
        let (realm, service, scope) = parse_www_authenticate(www_authenticate)?;

        // Use reqwest's query builder for proper URL encoding
        let token_url = reqwest::Url::parse_with_params(
            &realm,
            &[("service", &service), ("scope", &scope)],
        )
        .map_err(|e| Error::NetworkFailure {
            message: format!("failed to construct token URL: {e}"),
        })?;

        let response = self
            .client
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

        let token_response: TokenResponse = response.json().await.map_err(|e| Error::NetworkFailure {
            message: format!("failed to parse token response: {e}"),
        })?;

        Ok(token_response.token)
    }

    async fn download_response_with_progress(
        &self,
        response: reqwest::Response,
        expected_sha256: &str,
        name: Option<String>,
        progress: Option<DownloadProgressCallback>,
    ) -> Result<PathBuf, Error> {
        // Get content length for progress tracking
        let total_bytes = response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());

        // Report download started
        if let (Some(cb), Some(n)) = (&progress, &name) {
            cb(InstallProgress::DownloadStarted {
                name: n.clone(),
                total_bytes,
            });
        }

        let mut writer = self
            .blob_cache
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
            writer.write_all(&chunk).map_err(|e| Error::NetworkFailure {
                message: format!("failed to write chunk: {e}"),
            })?;

            // Report progress
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

        // Report download completed
        if let (Some(cb), Some(n)) = (&progress, &name) {
            cb(InstallProgress::DownloadCompleted {
                name: n.clone(),
                total_bytes: downloaded,
            });
        }

        writer.commit()
    }
}

fn parse_www_authenticate(header: &str) -> Result<(String, String, String), Error> {
    let header = header.strip_prefix("Bearer ").ok_or_else(|| Error::NetworkFailure {
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
        let _permit = semaphore.acquire().await.map_err(|e| Error::NetworkFailure {
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

        let blob_path = tmp.path().join("blobs").join(format!("{wrong_sha256}.tar.gz"));
        assert!(!blob_path.exists());

        let tmp_path = tmp.path().join("tmp").join(format!("{wrong_sha256}.tar.gz.part"));
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
        assert!(peak <= 2, "peak concurrent downloads was {peak}, expected <= 2");
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
