use std::path::{Path, PathBuf};

use tokio::fs;
use zb_core::Error;

use crate::checksum::verify_sha256_bytes;
use crate::extraction::extract_tarball;

pub async fn download_and_extract_source(
    url: &str,
    expected_checksum: Option<&str>,
    work_dir: &Path,
) -> Result<PathBuf, Error> {
    let tarball_path = work_dir.join("source.tar.gz");
    download_source(url, &tarball_path).await?;

    verify_checksum(&tarball_path, expected_checksum, url).await?;

    let src_dir = work_dir.join("src");
    fs::create_dir_all(&src_dir)
        .await
        .map_err(Error::file("failed to create source directory"))?;

    extract_tarball(&tarball_path, &src_dir)?;

    find_source_root(&src_dir).await
}

async fn download_source(url: &str, dest: &Path) -> Result<(), Error> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(Error::network("failed to create HTTP client"))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(Error::network("failed to download source"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(Error::NetworkFailure {
            message: format!("source download returned HTTP {status}"),
        });
    }

    let bytes = response
        .bytes()
        .await
        .map_err(Error::network("failed to read source response"))?;

    fs::write(dest, &bytes)
        .await
        .map_err(Error::file("failed to write source tarball"))
}

async fn verify_checksum(path: &Path, expected: Option<&str>, url: &str) -> Result<(), Error> {
    let bytes = fs::read(path)
        .await
        .map_err(Error::file("failed to read tarball for checksum"))?;

    verify_sha256_bytes(&bytes, expected).map_err(|e| match e {
        Error::ChecksumMismatch { .. } => e,
        Error::InvalidArgument { message } => Error::InvalidArgument {
            message: format!("invalid source checksum for '{url}': {message}"),
        },
        other => other,
    })
}

async fn find_source_root(src_dir: &Path) -> Result<PathBuf, Error> {
    let mut entries = fs::read_dir(src_dir)
        .await
        .map_err(Error::file("failed to read source directory"))?;

    let mut subdirs = Vec::new();
    let mut has_files = false;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(Error::file("failed to read directory entry"))?
    {
        let ft = entry
            .file_type()
            .await
            .map_err(Error::file("failed to get file type"))?;
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
