use std::io::Write;
use std::path::PathBuf;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};

use crate::blob::BlobCache;
use zb_core::Error;

pub struct Downloader {
    client: reqwest::Client,
    blob_cache: BlobCache,
}

impl Downloader {
    pub fn new(blob_cache: BlobCache) -> Self {
        Self {
            client: reqwest::Client::new(),
            blob_cache,
        }
    }

    pub async fn download(
        &self,
        url: &str,
        expected_sha256: &str,
    ) -> Result<PathBuf, Error> {
        if self.blob_cache.has_blob(expected_sha256) {
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

        if !response.status().is_success() {
            return Err(Error::NetworkFailure {
                message: format!("HTTP {}", response.status()),
            });
        }

        let mut writer = self.blob_cache.start_write(expected_sha256).map_err(|e| {
            Error::NetworkFailure {
                message: format!("failed to create blob writer: {e}"),
            }
        })?;

        let mut hasher = Sha256::new();
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| Error::NetworkFailure {
                message: format!("failed to read chunk: {e}"),
            })?;

            hasher.update(&chunk);
            writer.write_all(&chunk).map_err(|e| Error::NetworkFailure {
                message: format!("failed to write chunk: {e}"),
            })?;
        }

        let actual_hash = format!("{:x}", hasher.finalize());

        if actual_hash != expected_sha256 {
            // Writer will be dropped without commit, cleaning up temp file
            return Err(Error::ChecksumMismatch {
                expected: expected_sha256.to_string(),
                actual: actual_hash,
            });
        }

        writer.commit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

        // Verify no blob was created
        let blob_path = tmp.path().join("blobs").join(format!("{wrong_sha256}.tar.gz"));
        assert!(!blob_path.exists());

        // Verify no temp file left behind
        let tmp_path = tmp.path().join("tmp").join(format!("{wrong_sha256}.tar.gz.part"));
        assert!(!tmp_path.exists());
    }

    #[tokio::test]
    async fn skips_download_if_blob_exists() {
        let mock_server = MockServer::start().await;
        let content = b"hello world";
        let sha256 = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

        // Set up mock but expect 0 calls
        Mock::given(method("GET"))
            .and(path("/test.tar.gz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(content.to_vec()))
            .expect(0)
            .mount(&mock_server)
            .await;

        let tmp = TempDir::new().unwrap();
        let blob_cache = BlobCache::new(tmp.path()).unwrap();

        // Pre-create the blob
        let mut writer = blob_cache.start_write(sha256).unwrap();
        writer.write_all(content).unwrap();
        writer.commit().unwrap();

        let downloader = Downloader::new(blob_cache);
        let url = format!("{}/test.tar.gz", mock_server.uri());
        let result = downloader.download(&url, sha256).await;

        assert!(result.is_ok());
        // Mock expectation of 0 calls will be verified when mock_server is dropped
    }
}
