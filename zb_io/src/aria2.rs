//! aria2c download backend for zerobrew.
//!
//! This module provides an optional aria2c-based downloader that can be used instead of
//! the built-in reqwest-based downloader. aria2c provides:
//! - Multi-connection downloads (segmented downloading)
//! - Better handling of slow/unreliable connections
//! - Resume support
//!
//! The aria2c backend is used by default if aria2c is detected in PATH.
//! To disable aria2c and force use of the built-in downloader, set `ZB_DISABLE_ARIA2=1`.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Semaphore, mpsc};

use crate::blob::BlobCache;
use crate::download::{
    DownloadBackend, DownloadProgressCallback, DownloadRequest, DownloadResult, GhcrTokenFetcher,
};
use crate::progress::InstallProgress;
use zb_core::Error;

/// Check if aria2c should be used.
/// Returns true by default unless ZB_DISABLE_ARIA2 is set to "1" or "true".
pub fn should_use_aria2() -> bool {
    std::env::var("ZB_DISABLE_ARIA2")
        .map(|v| !(v == "1" || v.to_lowercase() == "true"))
        .unwrap_or(true)
}

/// Detect aria2c in PATH
pub fn detect_aria2c() -> Option<PathBuf> {
    // Check common locations
    let candidates = [
        // Check PATH first via `which`
        None,
        // Common homebrew locations
        Some("/opt/homebrew/bin/aria2c"),
        Some("/usr/local/bin/aria2c"),
        // System locations
        Some("/usr/bin/aria2c"),
    ];

    for candidate in candidates {
        match candidate {
            None => {
                // Try `which aria2c`
                if let Ok(output) = Command::new("which").arg("aria2c").output()
                    && output.status.success()
                {
                    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !path.is_empty() {
                        return Some(PathBuf::from(path));
                    }
                }
            }
            Some(path) => {
                let p = PathBuf::from(path);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }

    None
}

/// JSON-RPC request structure
#[derive(Serialize)]
struct RpcRequest {
    jsonrpc: &'static str,
    id: String,
    method: String,
    params: Vec<serde_json::Value>,
}

/// JSON-RPC response structure
#[derive(Deserialize)]
struct RpcResponse {
    #[allow(dead_code)]
    id: String,
    result: Option<serde_json::Value>,
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

/// aria2c download status
#[allow(dead_code)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Aria2Status {
    gid: String,
    status: String,
    total_length: String,
    completed_length: String,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
}

/// aria2c RPC client
struct Aria2Client {
    http_client: reqwest::Client,
    rpc_url: String,
    secret: String,
    request_id: AtomicU64,
}

impl Aria2Client {
    fn new(port: u16, secret: &str) -> Self {
        Self {
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            rpc_url: format!("http://127.0.0.1:{}/jsonrpc", port),
            secret: secret.to_string(),
            request_id: AtomicU64::new(1),
        }
    }

    fn next_id(&self) -> String {
        self.request_id.fetch_add(1, Ordering::Relaxed).to_string()
    }

    async fn call(
        &self,
        method: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value, Error> {
        let mut full_params = vec![serde_json::Value::String(format!("token:{}", self.secret))];
        full_params.extend(params);

        let request = RpcRequest {
            jsonrpc: "2.0",
            id: self.next_id(),
            method: method.to_string(),
            params: full_params,
        };

        let response = self
            .http_client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::NetworkFailure {
                message: format!("aria2 RPC request failed: {e}"),
            })?;

        let rpc_response: RpcResponse =
            response.json().await.map_err(|e| Error::NetworkFailure {
                message: format!("aria2 RPC response parse failed: {e}"),
            })?;

        if let Some(error) = rpc_response.error {
            return Err(Error::NetworkFailure {
                message: format!("aria2 RPC error {}: {}", error.code, error.message),
            });
        }

        rpc_response.result.ok_or_else(|| Error::NetworkFailure {
            message: "aria2 RPC returned no result".to_string(),
        })
    }

    /// Add a download to aria2c
    async fn add_uri(
        &self,
        url: &str,
        output_path: &str,
        headers: &[(&str, &str)],
    ) -> Result<String, Error> {
        let mut options: HashMap<String, String> = HashMap::new();
        options.insert("out".to_string(), output_path.to_string());
        options.insert("auto-file-renaming".to_string(), "false".to_string());
        options.insert("allow-overwrite".to_string(), "true".to_string());

        // Add headers
        let header_strings: Vec<String> = headers
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect();
        if !header_strings.is_empty() {
            options.insert("header".to_string(), header_strings.join("\n"));
        }

        let result = self
            .call(
                "aria2.addUri",
                vec![
                    serde_json::json!([url]),
                    serde_json::to_value(&options).unwrap(),
                ],
            )
            .await?;

        result
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| Error::NetworkFailure {
                message: "aria2 addUri returned invalid GID".to_string(),
            })
    }

    /// Get status of a download
    async fn tell_status(&self, gid: &str) -> Result<Aria2Status, Error> {
        let result = self
            .call("aria2.tellStatus", vec![serde_json::json!(gid)])
            .await?;

        serde_json::from_value(result).map_err(|e| Error::NetworkFailure {
            message: format!("aria2 tellStatus parse failed: {e}"),
        })
    }

    /// Remove a download
    #[allow(dead_code)]
    async fn remove(&self, gid: &str) -> Result<(), Error> {
        self.call("aria2.remove", vec![serde_json::json!(gid)])
            .await?;
        Ok(())
    }

    /// Shutdown aria2c gracefully
    async fn shutdown(&self) -> Result<(), Error> {
        self.call("aria2.shutdown", vec![]).await?;
        Ok(())
    }
}

/// aria2c download backend
pub struct Aria2Downloader {
    #[allow(dead_code)]
    aria2c_path: PathBuf,
    client: Arc<Aria2Client>,
    blob_cache: BlobCache,
    token_fetcher: GhcrTokenFetcher,
    semaphore: Arc<Semaphore>,
    #[allow(dead_code)]
    child: Arc<Mutex<Option<Child>>>,
    tmp_dir: PathBuf,
}

impl Aria2Downloader {
    /// Create a new aria2c downloader, starting the aria2c daemon
    pub async fn new(aria2c_path: PathBuf, blob_cache: BlobCache) -> Result<Self, Error> {
        let port = rand::thread_rng().gen_range(49152..65535);
        let secret: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        // Create temp directory for downloads
        let tmp_dir = blob_cache.tmp_dir();

        // Start aria2c daemon
        let child = Command::new(&aria2c_path)
            .args([
                "--enable-rpc=true",
                &format!("--rpc-listen-port={}", port),
                &format!("--rpc-secret={}", secret),
                "--rpc-listen-all=false",
                "--daemon=false",
                "--quiet=true",
                "--check-certificate=true",
                "--max-concurrent-downloads=8",
                "--max-connection-per-server=4",
                "--split=4",
                "--min-split-size=1M",
                "--continue=true",
                "--auto-file-renaming=false",
                "--allow-overwrite=true",
                &format!("--dir={}", tmp_dir.display()),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| Error::NetworkFailure {
                message: format!("failed to start aria2c: {e}"),
            })?;

        let client = Arc::new(Aria2Client::new(port, &secret));

        // Wait for aria2c to be ready
        let mut attempts = 0;
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            attempts += 1;

            if attempts > 50 {
                return Err(Error::NetworkFailure {
                    message: "aria2c failed to start within 5 seconds".to_string(),
                });
            }

            // Try to call getVersion to verify it's running
            if client.call("aria2.getVersion", vec![]).await.is_ok() {
                break;
            }
        }

        Ok(Self {
            aria2c_path,
            client,
            blob_cache,
            token_fetcher: GhcrTokenFetcher::new(),
            semaphore: Arc::new(Semaphore::new(8)),
            child: Arc::new(Mutex::new(Some(child))),
            tmp_dir,
        })
    }

    /// Download a single file using aria2c
    async fn download_file(
        &self,
        url: &str,
        expected_sha256: &str,
        name: Option<String>,
        progress: Option<DownloadProgressCallback>,
    ) -> Result<PathBuf, Error> {
        // Check if blob already exists
        if self.blob_cache.has_blob(expected_sha256) {
            if let (Some(cb), Some(n)) = (&progress, &name) {
                cb(InstallProgress::DownloadCompleted {
                    name: n.clone(),
                    total_bytes: 0,
                });
            }
            return Ok(self.blob_cache.blob_path(expected_sha256));
        }

        // Get auth token for GHCR URLs
        let token = self.token_fetcher.get_token_for_url(url).await?;

        let mut headers: Vec<(&str, String)> = vec![("User-Agent", "zerobrew/0.1".to_string())];
        if let Some(ref t) = token {
            headers.push(("Authorization", format!("Bearer {}", t)));
        }

        // Output to temp file
        let temp_filename = format!("{}.tar.gz.part", expected_sha256);
        let temp_path = self.tmp_dir.join(&temp_filename);

        // Convert headers to references
        let header_refs: Vec<(&str, &str)> =
            headers.iter().map(|(k, v)| (*k, v.as_str())).collect();

        // Add download to aria2c
        let gid = self
            .client
            .add_uri(url, &temp_filename, &header_refs)
            .await?;

        if let (Some(cb), Some(n)) = (&progress, &name) {
            cb(InstallProgress::DownloadStarted {
                name: n.clone(),
                total_bytes: None,
            });
        }

        // Poll for completion
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let status = self.client.tell_status(&gid).await?;

            let total: u64 = status.total_length.parse().unwrap_or(0);
            let completed: u64 = status.completed_length.parse().unwrap_or(0);

            if let (Some(cb), Some(n)) = (&progress, &name) {
                cb(InstallProgress::DownloadProgress {
                    name: n.clone(),
                    downloaded: completed,
                    total_bytes: if total > 0 { Some(total) } else { None },
                });
            }

            match status.status.as_str() {
                "complete" => break,
                "error" => {
                    let msg = status
                        .error_message
                        .unwrap_or_else(|| "unknown error".to_string());
                    return Err(Error::NetworkFailure {
                        message: format!("aria2 download failed: {}", msg),
                    });
                }
                "removed" => {
                    return Err(Error::NetworkFailure {
                        message: "download was removed".to_string(),
                    });
                }
                _ => {} // active, waiting, paused - continue polling
            }
        }

        // Verify checksum
        let content = std::fs::read(&temp_path).map_err(|e| Error::NetworkFailure {
            message: format!("failed to read downloaded file: {e}"),
        })?;

        let mut hasher = Sha256::new();
        hasher.update(&content);
        let actual_hash = format!("{:x}", hasher.finalize());

        if actual_hash != expected_sha256 {
            let _ = std::fs::remove_file(&temp_path);
            return Err(Error::ChecksumMismatch {
                expected: expected_sha256.to_string(),
                actual: actual_hash,
            });
        }

        // Move to blob cache
        let mut writer =
            self.blob_cache
                .start_write(expected_sha256)
                .map_err(|e| Error::NetworkFailure {
                    message: format!("failed to create blob writer: {e}"),
                })?;
        writer
            .write_all(&content)
            .map_err(|e| Error::NetworkFailure {
                message: format!("failed to write blob: {e}"),
            })?;
        let blob_path = writer.commit()?;

        // Clean up temp file
        let _ = std::fs::remove_file(&temp_path);

        if let (Some(cb), Some(n)) = (&progress, &name) {
            cb(InstallProgress::DownloadCompleted {
                name: n.clone(),
                total_bytes: content.len() as u64,
            });
        }

        Ok(blob_path)
    }
}

impl Drop for Aria2Downloader {
    fn drop(&mut self) {
        // Try to shutdown aria2c gracefully
        let client = self.client.clone();
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(rt) = rt {
                let _ = rt.block_on(client.shutdown());
            }
        })
        .join();
    }
}

#[async_trait]
impl DownloadBackend for Aria2Downloader {
    fn download_streaming(
        &self,
        requests: Vec<DownloadRequest>,
        progress: Option<DownloadProgressCallback>,
    ) -> mpsc::Receiver<Result<DownloadResult, Error>> {
        let (tx, rx) = mpsc::channel(requests.len().max(1));

        for (index, req) in requests.into_iter().enumerate() {
            let semaphore = self.semaphore.clone();
            let client = self.client.clone();
            let blob_cache = self.blob_cache.clone();
            let token_fetcher = self.token_fetcher.clone();
            let tmp_dir = self.tmp_dir.clone();
            let progress = progress.clone();
            let tx = tx.clone();
            let name = req.name.clone();
            let sha256 = req.sha256.clone();

            tokio::spawn(async move {
                let _permit = semaphore.acquire().await;

                let result = download_file_internal(
                    &client,
                    &blob_cache,
                    &token_fetcher,
                    &tmp_dir,
                    &req.url,
                    &req.sha256,
                    Some(req.name.clone()),
                    progress,
                )
                .await;

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

    async fn download_single(
        &self,
        request: DownloadRequest,
        progress: Option<DownloadProgressCallback>,
    ) -> Result<PathBuf, Error> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| Error::NetworkFailure {
                message: format!("semaphore error: {e}"),
            })?;

        self.download_file(&request.url, &request.sha256, Some(request.name), progress)
            .await
    }

    fn remove_blob(&self, sha256: &str) -> bool {
        self.blob_cache.remove_blob(sha256).unwrap_or(false)
    }
}

/// Internal download function for use in spawned tasks
#[allow(clippy::too_many_arguments)]
async fn download_file_internal(
    client: &Aria2Client,
    blob_cache: &BlobCache,
    token_fetcher: &GhcrTokenFetcher,
    tmp_dir: &std::path::Path,
    url: &str,
    expected_sha256: &str,
    name: Option<String>,
    progress: Option<DownloadProgressCallback>,
) -> Result<PathBuf, Error> {
    // Check if blob already exists
    if blob_cache.has_blob(expected_sha256) {
        if let (Some(cb), Some(n)) = (&progress, &name) {
            cb(InstallProgress::DownloadCompleted {
                name: n.clone(),
                total_bytes: 0,
            });
        }
        return Ok(blob_cache.blob_path(expected_sha256));
    }

    // Get auth token for GHCR URLs
    let token = token_fetcher.get_token_for_url(url).await?;

    let mut headers: Vec<(&str, String)> = vec![("User-Agent", "zerobrew/0.1".to_string())];
    if let Some(ref t) = token {
        headers.push(("Authorization", format!("Bearer {}", t)));
    }

    // Output to temp file
    let temp_filename = format!("{}.tar.gz.part", expected_sha256);
    let temp_path = tmp_dir.join(&temp_filename);

    // Convert headers to references
    let header_refs: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();

    // Add download to aria2c
    let gid = client.add_uri(url, &temp_filename, &header_refs).await?;

    if let (Some(cb), Some(n)) = (&progress, &name) {
        cb(InstallProgress::DownloadStarted {
            name: n.clone(),
            total_bytes: None,
        });
    }

    // Poll for completion
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;

        let status = client.tell_status(&gid).await?;

        let total: u64 = status.total_length.parse().unwrap_or(0);
        let completed: u64 = status.completed_length.parse().unwrap_or(0);

        if let (Some(cb), Some(n)) = (&progress, &name) {
            cb(InstallProgress::DownloadProgress {
                name: n.clone(),
                downloaded: completed,
                total_bytes: if total > 0 { Some(total) } else { None },
            });
        }

        match status.status.as_str() {
            "complete" => break,
            "error" => {
                let msg = status
                    .error_message
                    .unwrap_or_else(|| "unknown error".to_string());
                return Err(Error::NetworkFailure {
                    message: format!("aria2 download failed: {}", msg),
                });
            }
            "removed" => {
                return Err(Error::NetworkFailure {
                    message: "download was removed".to_string(),
                });
            }
            _ => {} // active, waiting, paused - continue polling
        }
    }

    // Verify checksum
    let content = std::fs::read(&temp_path).map_err(|e| Error::NetworkFailure {
        message: format!("failed to read downloaded file: {e}"),
    })?;

    let mut hasher = Sha256::new();
    hasher.update(&content);
    let actual_hash = format!("{:x}", hasher.finalize());

    if actual_hash != expected_sha256 {
        let _ = std::fs::remove_file(&temp_path);
        return Err(Error::ChecksumMismatch {
            expected: expected_sha256.to_string(),
            actual: actual_hash,
        });
    }

    // Move to blob cache
    let mut writer =
        blob_cache
            .start_write(expected_sha256)
            .map_err(|e| Error::NetworkFailure {
                message: format!("failed to create blob writer: {e}"),
            })?;
    writer
        .write_all(&content)
        .map_err(|e| Error::NetworkFailure {
            message: format!("failed to write blob: {e}"),
        })?;
    let blob_path = writer.commit()?;

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_path);

    if let (Some(cb), Some(n)) = (&progress, &name) {
        cb(InstallProgress::DownloadCompleted {
            name: n.clone(),
            total_bytes: content.len() as u64,
        });
    }

    Ok(blob_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test all env var scenarios in a single test to avoid race conditions
    /// (env vars are global state shared across parallel test threads)
    #[test]
    fn test_should_use_aria2_env_var() {
        // SAFETY: We run all env var tests sequentially in one test to avoid races
        unsafe {
            // Test default (unset) - should default to true
            std::env::remove_var("ZB_DISABLE_ARIA2");
            assert!(
                should_use_aria2(),
                "should be true when unset (default enabled)"
            );

            // Test "1" (disabled)
            std::env::set_var("ZB_DISABLE_ARIA2", "1");
            assert!(!should_use_aria2(), "should be false when set to '1'");

            // Test "true" (disabled)
            std::env::set_var("ZB_DISABLE_ARIA2", "true");
            assert!(!should_use_aria2(), "should be false when set to 'true'");

            // Test "TRUE" (case insensitive, disabled)
            std::env::set_var("ZB_DISABLE_ARIA2", "TRUE");
            assert!(!should_use_aria2(), "should be false when set to 'TRUE'");

            // Test "0" (not disabled, aria2 enabled)
            std::env::set_var("ZB_DISABLE_ARIA2", "0");
            assert!(should_use_aria2(), "should be true when set to '0'");

            // Test empty string (not disabled, aria2 enabled)
            std::env::set_var("ZB_DISABLE_ARIA2", "");
            assert!(should_use_aria2(), "should be true when empty");

            // Clean up
            std::env::remove_var("ZB_DISABLE_ARIA2");
        }
    }

    #[test]
    fn test_detect_aria2c() {
        // This test just verifies the function doesn't panic
        // It may or may not find aria2c depending on the system
        let _ = detect_aria2c();
    }
}
