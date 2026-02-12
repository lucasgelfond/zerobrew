use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::fs;
use zb_core::Error;

use crate::extraction::extract_tarball;

pub async fn download_and_extract_source(
    url: &str,
    expected_checksum: Option<&str>,
    work_dir: &Path,
) -> Result<PathBuf, Error> {
    let tarball_path = work_dir.join("source.tar.gz");
    download_source(url, &tarball_path).await?;

    if let Some(expected) = expected_checksum {
        verify_checksum(&tarball_path, expected).await?;
    }

    let src_dir = work_dir.join("src");
    fs::create_dir_all(&src_dir)
        .await
        .map_err(|e| Error::FileError {
            message: format!("failed to create source directory: {e}"),
        })?;

    extract_tarball(&tarball_path, &src_dir)?;

    find_source_root(&src_dir).await
}

async fn download_source(url: &str, dest: &Path) -> Result<(), Error> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| Error::NetworkFailure {
            message: format!("failed to create HTTP client: {e}"),
        })?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::NetworkFailure {
            message: format!("failed to download source: {e}"),
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(Error::NetworkFailure {
            message: format!("source download returned HTTP {status}"),
        });
    }

    let bytes = response.bytes().await.map_err(|e| Error::NetworkFailure {
        message: format!("failed to read source response: {e}"),
    })?;

    fs::write(dest, &bytes).await.map_err(|e| Error::FileError {
        message: format!("failed to write source tarball: {e}"),
    })
}

async fn verify_checksum(path: &Path, expected: &str) -> Result<(), Error> {
    let bytes = fs::read(path).await.map_err(|e| Error::FileError {
        message: format!("failed to read tarball for checksum: {e}"),
    })?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual = format!("{:x}", hasher.finalize());

    if actual != expected.to_lowercase() {
        return Err(Error::ChecksumMismatch {
            expected: expected.to_string(),
            actual,
        });
    }

    Ok(())
}

async fn find_source_root(src_dir: &Path) -> Result<PathBuf, Error> {
    let mut entries = fs::read_dir(src_dir).await.map_err(|e| Error::FileError {
        message: format!("failed to read source directory: {e}"),
    })?;

    let mut subdirs = Vec::new();
    let mut has_files = false;

    while let Some(entry) = entries.next_entry().await.map_err(|e| Error::FileError {
        message: format!("failed to read directory entry: {e}"),
    })? {
        let ft = entry.file_type().await.map_err(|e| Error::FileError {
            message: format!("failed to get file type: {e}"),
        })?;
        if ft.is_dir() {
            subdirs.push(entry.path());
        } else {
            has_files = true;
        }
    }

    if subdirs.len() == 1 && !has_files {
        return Ok(subdirs.into_iter().next().unwrap());
    }

    Ok(src_dir.to_path_buf())
}
