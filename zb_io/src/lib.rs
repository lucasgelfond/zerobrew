// Submodules
pub mod cellar;
pub mod extraction;
pub mod installer;
pub mod network;
pub mod progress;
pub mod ssl;
pub mod storage;

// Re-exports for convenience
pub use cellar::{Cellar, LinkedFile, Linker};
pub use extraction::extract_tarball;
pub use installer::{
    ExecuteResult, HomebrewMigrationPackages, HomebrewPackage, InstallPlan, Installer,
    create_installer, get_homebrew_packages,
};
pub use network::{
    ApiCache, ApiClient, DownloadProgressCallback, DownloadRequest, Downloader, ParallelDownloader,
};
pub use progress::{InstallProgress, ProgressCallback};
pub use ssl::{find_ca_bundle_from_prefix, find_ca_dir};
pub use storage::{BlobCache, Database, InstalledKeg, Store};
