use rusqlite::{Connection, params};
use std::path::Path;

pub struct ApiCache {
    conn: Connection,
}

impl std::fmt::Debug for ApiCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiCache").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub body: String,
}

impl ApiCache {
    const SCHEMA_VERSION: u32 = 1;

    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    fn get_schema_version(conn: &Connection) -> Result<u32, rusqlite::Error> {
        let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        Ok(version)
    }

    fn set_schema_version(conn: &Connection, version: u32) -> Result<(), rusqlite::Error> {
        conn.execute(&format!("PRAGMA user_version = {}", version), [])?;
        Ok(())
    }

    fn migrate(conn: &Connection) -> Result<(), rusqlite::Error> {
        let current_version = Self::get_schema_version(conn)?;

        if current_version > Self::SCHEMA_VERSION {
            return Err(rusqlite::Error::InvalidQuery);
        }

        if current_version == Self::SCHEMA_VERSION {
            return Ok(());
        }

        for version in current_version..Self::SCHEMA_VERSION {
            let next_version = version + 1;
            Self::migrate_to_version(conn, next_version)?;
            Self::set_schema_version(conn, next_version)?;
        }

        Ok(())
    }

    fn migrate_to_version(conn: &Connection, version: u32) -> Result<(), rusqlite::Error> {
        match version {
            1 => Self::migrate_to_v1(conn),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }

    fn migrate_to_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
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

    /// Clear all cached entries. Returns the number of entries removed.
    pub fn clear(&self) -> Result<usize, rusqlite::Error> {
        let removed = self.conn.execute("DELETE FROM api_cache", [])?;
        Ok(removed)
    }

    pub fn put(&self, url: &str, entry: &CacheEntry) -> Result<(), rusqlite::Error> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

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
    fn clear_removes_all_entries() {
        let cache = ApiCache::in_memory().unwrap();
        let entry = CacheEntry {
            etag: None,
            last_modified: None,
            body: "{}".to_string(),
        };
        cache.put("https://example.com/a.json", &entry).unwrap();
        cache.put("https://example.com/b.json", &entry).unwrap();

        let removed = cache.clear().unwrap();
        assert_eq!(removed, 2);
        assert!(cache.get("https://example.com/a.json").is_none());
        assert!(cache.get("https://example.com/b.json").is_none());
    }

    #[test]
    fn clear_on_empty_cache_returns_zero() {
        let cache = ApiCache::in_memory().unwrap();
        assert_eq!(cache.clear().unwrap(), 0);
    }

    #[test]
    fn new_database_starts_at_version_1() {
        let cache = ApiCache::in_memory().expect("failed to create cache");
        let version = ApiCache::get_schema_version(&cache.conn).expect("failed to get version");
        assert_eq!(version, 1);
    }

    #[test]
    fn migration_is_idempotent() {
        let cache = ApiCache::in_memory().expect("failed to create cache");
        ApiCache::migrate(&cache.conn).expect("first migration failed");
        ApiCache::migrate(&cache.conn).expect("second migration failed");
        let version = ApiCache::get_schema_version(&cache.conn).expect("failed to get version");
        assert_eq!(version, 1);
    }

    #[test]
    fn rejects_future_schema_version() {
        let conn = Connection::open_in_memory().expect("failed to open connection");
        ApiCache::set_schema_version(&conn, 999).expect("failed to set version");
        let err = ApiCache::migrate(&conn).unwrap_err();
        assert!(matches!(err, rusqlite::Error::InvalidQuery));
    }

    #[test]
    fn migration_preserves_existing_data() {
        let conn = Connection::open_in_memory().expect("failed to open connection");

        conn.execute(
            "CREATE TABLE api_cache (
                url TEXT PRIMARY KEY,
                etag TEXT,
                last_modified TEXT,
                body TEXT NOT NULL,
                cached_at INTEGER NOT NULL
            )",
            [],
        )
        .expect("failed to create old schema");

        conn.execute(
            "INSERT INTO api_cache VALUES ('https://example.com', 'abc', NULL, 'data', 123)",
            [],
        )
        .expect("failed to insert test data");

        ApiCache::migrate(&conn).expect("migration failed");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM api_cache", [], |row| row.get(0))
            .expect("failed to count rows");
        assert_eq!(count, 1);

        let url: String = conn
            .query_row("SELECT url FROM api_cache", [], |row| row.get(0))
            .expect("failed to query data");
        assert_eq!(url, "https://example.com");
    }
}
