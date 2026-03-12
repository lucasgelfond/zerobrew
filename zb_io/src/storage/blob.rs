use std::fs;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;
use zb_core::Error;

#[derive(Clone)]
pub struct BlobCache {
    blobs_dir: PathBuf,
    tmp_dir: PathBuf,
}

impl BlobCache {
    pub fn new(cache_root: &Path) -> io::Result<Self> {
        let blobs_dir = cache_root.join("blobs");
        let tmp_dir = cache_root.join("tmp");

        fs::create_dir_all(&blobs_dir)?;
        fs::create_dir_all(&tmp_dir)?;

        Ok(Self { blobs_dir, tmp_dir })
    }

    pub fn blob_path(&self, sha256: &str) -> PathBuf {
        self.blobs_dir.join(format!("{sha256}.tar.gz"))
    }

    pub fn has_blob(&self, sha256: &str) -> bool {
        self.blob_path(sha256).exists()
    }

    /// Remove a blob from the cache (used when extraction fails due to corruption)
    pub fn remove_blob(&self, sha256: &str) -> io::Result<bool> {
        let path = self.blob_path(sha256);
        if path.exists() {
            fs::remove_file(&path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn start_write(&self, sha256: &str) -> io::Result<BlobWriter> {
        let final_path = self.blob_path(sha256);
        let temp_file = NamedTempFile::new_in(&self.tmp_dir)?;
        Ok(BlobWriter {
            temp_file,
            final_path,
        })
    }
}

pub struct BlobWriter {
    temp_file: NamedTempFile,
    final_path: PathBuf,
}

impl BlobWriter {
    pub fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.temp_file.seek(pos)
    }

    pub fn commit(self) -> Result<PathBuf, Error> {
        // Content-addressed: same sha256 = identical content, so overwrite is safe.
        // NamedTempFile::persist does an atomic rename(2) on Unix.
        // On drop (e.g. if persist is never called), the temp file is auto-deleted.
        self.temp_file
            .persist(&self.final_path)
            .map_err(Error::store("failed to persist blob"))?;
        Ok(self.final_path)
    }
}

impl Write for BlobWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.temp_file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.temp_file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn completed_write_produces_final_blob() {
        let tmp = TempDir::new().unwrap();
        let cache = BlobCache::new(tmp.path()).unwrap();

        let sha = "abc123";
        let mut writer = cache.start_write(sha).unwrap();
        writer.write_all(b"hello world").unwrap();

        let final_path = writer.commit().unwrap();

        assert!(final_path.exists());
        assert!(cache.has_blob(sha));
        assert_eq!(fs::read_to_string(&final_path).unwrap(), "hello world");
    }

    #[test]
    fn interrupted_write_leaves_no_final_blob() {
        let tmp = TempDir::new().unwrap();
        let cache = BlobCache::new(tmp.path()).unwrap();

        let sha = "def456";

        {
            let mut writer = cache.start_write(sha).unwrap();
            writer.write_all(b"partial data").unwrap();
            // writer is dropped without calling commit()
        }

        // Final blob should not exist
        assert!(!cache.has_blob(sha));

        // Temp file should be cleaned up (temp files now have unique suffixes)
        let tmp_dir = tmp.path().join("tmp");
        let has_temp_files = fs::read_dir(&tmp_dir)
            .unwrap()
            .any(|e| e.unwrap().file_name().to_string_lossy().starts_with(sha));
        assert!(!has_temp_files, "temp files for {sha} should be cleaned up");
    }

    #[test]
    fn blob_path_uses_sha256() {
        let tmp = TempDir::new().unwrap();
        let cache = BlobCache::new(tmp.path()).unwrap();

        let path = cache.blob_path("deadbeef");
        assert!(path.to_string_lossy().contains("deadbeef.tar.gz"));
    }

    #[test]
    fn remove_blob_deletes_existing_blob() {
        let tmp = TempDir::new().unwrap();
        let cache = BlobCache::new(tmp.path()).unwrap();

        let sha = "removeme";
        let mut writer = cache.start_write(sha).unwrap();
        writer.write_all(b"corrupt data").unwrap();
        writer.commit().unwrap();

        assert!(cache.has_blob(sha));

        let removed = cache.remove_blob(sha).unwrap();
        assert!(removed);
        assert!(!cache.has_blob(sha));
    }

    #[test]
    fn remove_blob_returns_false_for_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let cache = BlobCache::new(tmp.path()).unwrap();

        let removed = cache.remove_blob("nonexistent").unwrap();
        assert!(!removed);
    }
}
