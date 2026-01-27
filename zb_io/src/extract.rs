use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use flate2::read::GzDecoder;
use tar::Archive;
use xz2::read::XzDecoder;
use zstd::stream::read::Decoder as ZstdDecoder;

use zb_core::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompressionFormat {
    Gzip,
    Xz,
    Zstd,
    Unknown,
}

fn detect_compression(path: &Path) -> Result<CompressionFormat, Error> {
    let mut file = File::open(path).map_err(|e| Error::StoreCorruption {
        message: format!("failed to open tarball: {e}"),
    })?;

    let mut magic = [0u8; 6];
    let bytes_read = file.read(&mut magic).map_err(|e| Error::StoreCorruption {
        message: format!("failed to read magic bytes: {e}"),
    })?;

    if bytes_read < 2 {
        return Ok(CompressionFormat::Unknown);
    }

    // Gzip: 1f 8b
    if magic[0] == 0x1f && magic[1] == 0x8b {
        return Ok(CompressionFormat::Gzip);
    }

    // XZ: fd 37 7a 58 5a 00 (FD 7zXZ\0)
    if bytes_read >= 6 && magic[0..6] == [0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00] {
        return Ok(CompressionFormat::Xz);
    }

    // Zstd: 28 b5 2f fd
    if bytes_read >= 4 && magic[0..4] == [0x28, 0xb5, 0x2f, 0xfd] {
        return Ok(CompressionFormat::Zstd);
    }

    Ok(CompressionFormat::Unknown)
}

pub fn extract_tarball(tarball_path: &Path, dest_dir: &Path) -> Result<(), Error> {
    let format = detect_compression(tarball_path)?;

    let file = File::open(tarball_path).map_err(|e| Error::StoreCorruption {
        message: format!("failed to open tarball: {e}"),
    })?;
    let reader = BufReader::new(file);

    match format {
        CompressionFormat::Gzip => {
            let decoder = GzDecoder::new(reader);
            extract_tar_archive(decoder, dest_dir)
        }
        CompressionFormat::Xz => {
            let decoder = XzDecoder::new(reader);
            extract_tar_archive(decoder, dest_dir)
        }
        CompressionFormat::Zstd => {
            let decoder = ZstdDecoder::new(reader).map_err(|e| Error::StoreCorruption {
                message: format!("failed to create zstd decoder: {e}"),
            })?;
            extract_tar_archive(decoder, dest_dir)
        }
        CompressionFormat::Unknown => {
            // Try gzip as fallback
            let decoder = GzDecoder::new(reader);
            extract_tar_archive(decoder, dest_dir)
        }
    }
}

fn extract_tar_archive<R: Read>(reader: R, dest_dir: &Path) -> Result<(), Error> {
    let mut archive = Archive::new(reader);

    archive.set_preserve_permissions(true);
    archive.set_unpack_xattrs(true);

    for entry in archive.entries().map_err(|e| Error::StoreCorruption {
        message: format!("failed to read archive entries: {e}"),
    })? {
        let mut entry = entry.map_err(|e| Error::StoreCorruption {
            message: format!("failed to read archive entry: {e}"),
        })?;

        let entry_path = entry.path().map_err(|e| Error::StoreCorruption {
            message: format!("failed to read entry path: {e}"),
        })?;

        // Security check: reject path traversal
        validate_path(&entry_path)?;

        // Store path as owned string for error message
        let path_display = entry_path.display().to_string();

        // Ensure the entry doesn't escape the destination directory
        let full_path = dest_dir.join(&*entry_path);
        let canonical_dest = dest_dir
            .canonicalize()
            .unwrap_or_else(|_| dest_dir.to_path_buf());
        if let Ok(canonical_full) = full_path.canonicalize()
            && !canonical_full.starts_with(&canonical_dest)
        {
            return Err(Error::StoreCorruption {
                message: format!("path traversal attempt: {path_display}"),
            });
        }

        entry
            .unpack_in(dest_dir)
            .map_err(|e| Error::StoreCorruption {
                message: format!("failed to unpack entry {path_display}: {e}"),
            })?;
    }

    Ok(())
}

fn validate_path(path: &Path) -> Result<(), Error> {
    // Reject absolute paths
    if path.is_absolute() {
        return Err(Error::StoreCorruption {
            message: format!("absolute path in archive: {}", path.display()),
        });
    }

    // Reject paths with .. components
    for component in path.components() {
        if let std::path::Component::ParentDir = component {
            return Err(Error::StoreCorruption {
                message: format!("path traversal in archive: {}", path.display()),
            });
        }
    }

    Ok(())
}

/// Extract a tarball from a reader (assumes gzip compression).
/// For file-based extraction with auto-detection, use `extract_tarball` instead.
pub fn extract_tarball_from_reader<R: Read>(reader: R, dest_dir: &Path) -> Result<(), Error> {
    let decoder = GzDecoder::new(reader);
    extract_tar_archive(decoder, dest_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use tar::Builder;
    use tempfile::TempDir;

    fn create_test_tarball(entries: Vec<(&str, &[u8], Option<u32>)>) -> Vec<u8> {
        let mut builder = Builder::new(Vec::new());

        for (path, content, mode) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_path(path).unwrap();
            header.set_size(content.len() as u64);
            header.set_mode(mode.unwrap_or(0o644));
            header.set_cksum();
            builder.append(&header, content).unwrap();
        }

        let tar_data = builder.into_inner().unwrap();

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&tar_data).unwrap();
        encoder.finish().unwrap()
    }

    fn create_tarball_with_symlink(name: &str, target: &str) -> Vec<u8> {
        let mut builder = Builder::new(Vec::new());

        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_path(name).unwrap();
        header.set_size(0);
        header.set_mode(0o777);
        header.set_cksum();

        builder.append_link(&mut header, name, target).unwrap();

        let tar_data = builder.into_inner().unwrap();

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&tar_data).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn extracts_file_with_content() {
        let tmp = TempDir::new().unwrap();
        let tarball = create_test_tarball(vec![("hello.txt", b"Hello, World!", None)]);

        let tarball_path = tmp.path().join("test.tar.gz");
        fs::write(&tarball_path, &tarball).unwrap();

        let dest = tmp.path().join("extracted");
        fs::create_dir(&dest).unwrap();

        extract_tarball(&tarball_path, &dest).unwrap();

        let content = fs::read_to_string(dest.join("hello.txt")).unwrap();
        assert_eq!(content, "Hello, World!");
    }

    #[test]
    fn preserves_executable_bit() {
        let tmp = TempDir::new().unwrap();
        let tarball = create_test_tarball(vec![("script.sh", b"#!/bin/sh\necho hi", Some(0o755))]);

        let tarball_path = tmp.path().join("test.tar.gz");
        fs::write(&tarball_path, &tarball).unwrap();

        let dest = tmp.path().join("extracted");
        fs::create_dir(&dest).unwrap();

        extract_tarball(&tarball_path, &dest).unwrap();

        let metadata = fs::metadata(dest.join("script.sh")).unwrap();
        let mode = metadata.permissions().mode();
        assert!(
            mode & 0o111 != 0,
            "executable bit not preserved: {:o}",
            mode
        );
    }

    #[test]
    fn preserves_symlink() {
        let tmp = TempDir::new().unwrap();
        let tarball = create_tarball_with_symlink("link", "target.txt");

        let tarball_path = tmp.path().join("test.tar.gz");
        fs::write(&tarball_path, &tarball).unwrap();

        let dest = tmp.path().join("extracted");
        fs::create_dir(&dest).unwrap();

        extract_tarball(&tarball_path, &dest).unwrap();

        let link_path = dest.join("link");
        assert!(
            link_path
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_link(&link_path).unwrap(),
            PathBuf::from("target.txt")
        );
    }

    fn create_malicious_tarball(path: &[u8]) -> Vec<u8> {
        // Manually construct a tar header with unsafe path
        let mut tar_data = vec![0u8; 512 + 512]; // header + one block of data

        // Copy path into header (bytes 0-99)
        let path_len = path.len().min(100);
        tar_data[..path_len].copy_from_slice(&path[..path_len]);

        // Set mode (bytes 100-107) - "0000644\0"
        tar_data[100..108].copy_from_slice(b"0000644\0");

        // Set uid (bytes 108-115) - "0000000\0"
        tar_data[108..116].copy_from_slice(b"0000000\0");

        // Set gid (bytes 116-123) - "0000000\0"
        tar_data[116..124].copy_from_slice(b"0000000\0");

        // Set size (bytes 124-135) - "00000000004\0" for 4 bytes
        tar_data[124..136].copy_from_slice(b"00000000004\0");

        // Set mtime (bytes 136-147) - "00000000000\0"
        tar_data[136..148].copy_from_slice(b"00000000000\0");

        // Set typeflag (byte 156) - '0' for regular file
        tar_data[156] = b'0';

        // Calculate checksum (bytes 148-155)
        // First set checksum field to spaces
        tar_data[148..156].copy_from_slice(b"        ");

        let checksum: u32 = tar_data[..512].iter().map(|&b| b as u32).sum();
        let checksum_str = format!("{:06o}\0 ", checksum);
        tar_data[148..156].copy_from_slice(checksum_str.as_bytes());

        // Add content "evil" + padding to 512 bytes
        tar_data[512..516].copy_from_slice(b"evil");

        // Compress with gzip
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&tar_data).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn rejects_path_traversal() {
        let tmp = TempDir::new().unwrap();

        let tarball = create_malicious_tarball(b"../evil.txt");

        let tarball_path = tmp.path().join("evil.tar.gz");
        fs::write(&tarball_path, &tarball).unwrap();

        let dest = tmp.path().join("extracted");
        fs::create_dir(&dest).unwrap();

        let result = extract_tarball(&tarball_path, &dest);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("path traversal"));
    }

    #[test]
    fn rejects_absolute_path() {
        let tmp = TempDir::new().unwrap();

        let tarball = create_malicious_tarball(b"/etc/passwd");

        let tarball_path = tmp.path().join("absolute.tar.gz");
        fs::write(&tarball_path, &tarball).unwrap();

        let dest = tmp.path().join("extracted");
        fs::create_dir(&dest).unwrap();

        let result = extract_tarball(&tarball_path, &dest);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("absolute path"));
    }
}
