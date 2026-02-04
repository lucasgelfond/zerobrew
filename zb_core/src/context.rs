use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub root: PathBuf,
    pub store: PathBuf,
    pub cellar: PathBuf,
    pub cache: PathBuf,
    pub db: PathBuf,
    pub locks: PathBuf,
}

impl Paths {
    pub fn from_root(root: PathBuf) -> Self {
        let store = root.join("store");
        let cellar = root.join("cellar");
        let cache = root.join("cache");
        let db = root.join("db").join("zb.sqlite3");
        let locks = root.join("locks");

        Self {
            root,
            store,
            cellar,
            cache,
            db,
            locks,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConcurrencyLimits {
    pub download: usize,
    pub unpack: usize,
    pub materialize: usize,
}

impl Default for ConcurrencyLimits {
    fn default() -> Self {
        Self {
            download: 20,
            unpack: 4,
            materialize: 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoggerHandle {
    pub level: LogLevel,
}

impl Default for LoggerHandle {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub paths: Paths,
    pub concurrency: ConcurrencyLimits,
    pub logger: LoggerHandle,
}

impl Context {
    pub fn from_defaults() -> Self {
        Self {
            paths: Paths::from_root(PathBuf::from("/opt/zerobrew")),
            concurrency: ConcurrencyLimits::default(),
            logger: LoggerHandle::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_defaults_sets_expected_paths() {
        let context = Context::from_defaults();

        assert_eq!(context.paths.root, PathBuf::from("/opt/zerobrew"));
        assert_eq!(
            context.paths.store,
            PathBuf::from("/opt/zerobrew").join("store")
        );
        assert_eq!(
            context.paths.cellar,
            PathBuf::from("/opt/zerobrew").join("cellar")
        );
        assert_eq!(
            context.paths.cache,
            PathBuf::from("/opt/zerobrew").join("cache")
        );
        assert_eq!(
            context.paths.db,
            PathBuf::from("/opt/zerobrew").join("db").join("zb.sqlite3")
        );
        assert_eq!(
            context.paths.locks,
            PathBuf::from("/opt/zerobrew").join("locks")
        );
    }
}
