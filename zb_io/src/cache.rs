use rusqlite::{Connection, params};
use std::path::Path;

pub struct ApiCache {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub body: String,
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

    /// Get cached entry only if it's still fresh (within max_age_secs of being cached).
    /// Returns None if entry doesn't exist or is stale.
    pub fn get_if_fresh(&self, url: &str, max_age_secs: u64) -> Option<CacheEntry> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let min_cached_at = now - max_age_secs as i64;

        self.conn
            .query_row(
                "SELECT etag, last_modified, body FROM api_cache WHERE url = ?1 AND cached_at >= ?2",
                params![url, min_cached_at],
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

    #[test]
    fn get_if_fresh_returns_entry_within_ttl() {
        let cache = ApiCache::in_memory().unwrap();

        let entry = CacheEntry {
            etag: Some("abc123".to_string()),
            last_modified: None,
            body: r#"{"name":"foo"}"#.to_string(),
        };

        cache.put("https://example.com/foo.json", &entry).unwrap();

        // Should return entry within 1 hour TTL
        let retrieved = cache.get_if_fresh("https://example.com/foo.json", 3600);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().body, r#"{"name":"foo"}"#);
    }

    #[test]
    fn get_if_fresh_returns_none_for_missing_entry() {
        let cache = ApiCache::in_memory().unwrap();

        // No entry was added, should return None
        let retrieved = cache.get_if_fresh("https://example.com/nonexistent.json", 3600);
        assert!(retrieved.is_none());
    }
}
