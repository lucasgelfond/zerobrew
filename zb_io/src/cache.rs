use rusqlite::{Connection, params};
use std::path::Path;

/// SQLite-backed cache for Homebrew API responses.
/// Stores formula JSON with ETag/Last-Modified for conditional requests.
pub struct ApiCache {
    conn: Connection,
}

/// A cached API response entry.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub body: String,
}

/// Statistics about the API cache.
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of cached entries
    pub entry_count: usize,
    /// Timestamp of oldest entry (Unix epoch seconds)
    pub oldest_entry: Option<i64>,
    /// Timestamp of newest entry (Unix epoch seconds)
    pub newest_entry: Option<i64>,
}

impl ApiCache {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    pub fn in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    fn init_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS api_cache (
                url TEXT PRIMARY KEY,
                etag TEXT,
                last_modified TEXT,
                body TEXT NOT NULL,
                cached_at INTEGER NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    pub fn get(&self, url: &str) -> Option<CacheEntry> {
        self.conn
            .query_row(
                "SELECT etag, last_modified, body FROM api_cache WHERE url = ?1",
                params![url],
                |row| {
                    Ok(CacheEntry {
                        etag: row.get(0)?,
                        last_modified: row.get(1)?,
                        body: row.get(2)?,
                    })
                },
            )
            .ok()
    }

    pub fn put(&self, url: &str, entry: &CacheEntry) -> Result<(), rusqlite::Error> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.conn.execute(
            "INSERT OR REPLACE INTO api_cache (url, etag, last_modified, body, cached_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![url, entry.etag, entry.last_modified, entry.body, now],
        )?;
        Ok(())
    }

    /// Clear all cached entries. Returns the number of entries removed.
    pub fn clear(&self) -> Result<usize, rusqlite::Error> {
        let removed = self.conn.execute("DELETE FROM api_cache", [])?;
        Ok(removed)
    }

    /// Get cache statistics.
    pub fn stats(&self) -> Result<CacheStats, rusqlite::Error> {
        let entry_count: usize =
            self.conn
                .query_row("SELECT COUNT(*) FROM api_cache", [], |row| row.get(0))?;

        let oldest_entry: Option<i64> =
            self.conn
                .query_row("SELECT MIN(cached_at) FROM api_cache", [], |row| row.get(0))?;

        let newest_entry: Option<i64> =
            self.conn
                .query_row("SELECT MAX(cached_at) FROM api_cache", [], |row| row.get(0))?;

        Ok(CacheStats {
            entry_count,
            oldest_entry,
            newest_entry,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_retrieves_cache_entry() {
        let cache = ApiCache::in_memory().unwrap();

        let entry = CacheEntry {
            etag: Some("abc123".to_string()),
            last_modified: None,
            body: r#"{"name":"foo"}"#.to_string(),
        };

        cache.put("https://example.com/foo.json", &entry).unwrap();
        let retrieved = cache.get("https://example.com/foo.json").unwrap();

        assert_eq!(retrieved.etag, Some("abc123".to_string()));
        assert_eq!(retrieved.body, r#"{"name":"foo"}"#);
    }

    #[test]
    fn returns_none_for_missing_entry() {
        let cache = ApiCache::in_memory().unwrap();
        assert!(cache.get("https://example.com/nonexistent.json").is_none());
    }

    fn make_entry() -> CacheEntry {
        CacheEntry {
            etag: Some("test".to_string()),
            last_modified: None,
            body: r#"{"test":true}"#.to_string(),
        }
    }

    #[test]
    fn clear_removes_all_entries() {
        let cache = ApiCache::in_memory().unwrap();

        cache
            .put("https://example.com/a.json", &make_entry())
            .unwrap();
        cache
            .put("https://example.com/b.json", &make_entry())
            .unwrap();

        let removed = cache.clear().unwrap();
        assert_eq!(removed, 2);
        assert!(cache.get("https://example.com/a.json").is_none());
        assert!(cache.get("https://example.com/b.json").is_none());
    }

    #[test]
    fn clear_returns_zero_on_empty_cache() {
        let cache = ApiCache::in_memory().unwrap();
        let removed = cache.clear().unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn stats_returns_correct_counts() {
        let cache = ApiCache::in_memory().unwrap();

        // Empty cache
        let stats = cache.stats().unwrap();
        assert_eq!(stats.entry_count, 0);
        assert!(stats.oldest_entry.is_none());
        assert!(stats.newest_entry.is_none());

        // Add entries
        cache
            .put("https://example.com/a.json", &make_entry())
            .unwrap();
        cache
            .put("https://example.com/b.json", &make_entry())
            .unwrap();

        let stats = cache.stats().unwrap();
        assert_eq!(stats.entry_count, 2);
        assert!(stats.oldest_entry.is_some());
        assert!(stats.newest_entry.is_some());
    }
}
