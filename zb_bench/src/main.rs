use clap::{Parser, Subcommand};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Instant;
use tar::Builder;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use zb_io::{ApiClient, BlobCache, Cellar, Database, Installer, Linker, Store};

#[derive(Parser)]
#[command(name = "zb-bench")]
#[command(about = "Zerobrew benchmarking tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run smoke benchmark with mocked API
    Smoke,
    /// Run real performance benchmark
    Real {
        /// Formula to benchmark (default: jq)
        #[arg(default_value = "jq")]
        formula: String,
    },
    /// Run benchmark suite against popular packages
    Suite {
        /// Number of top packages to test (default: 10)
        #[arg(short, long, default_value = "10")]
        count: usize,

        /// Use abridged list (fast packages with few deps)
        #[arg(long)]
        quick: bool,
    },
}

#[derive(Serialize)]
struct BenchResult {
    name: String,
    cold_install_ms: u64,
    warm_reinstall_ms: u64,
    speedup: f64,
}

#[derive(Serialize)]
struct SmokeResult {
    total_ms: u64,
    resolve_ms: u64,
    download_ms: u64,
    install_ms: u64,
    uninstall_ms: u64,
    formulas_count: usize,
}

fn create_bottle_tarball(formula_name: &str) -> Vec<u8> {
    let mut builder = Builder::new(Vec::new());

    // Create bin directory with executable
    let mut header = tar::Header::new_gnu();
    header
        .set_path(format!("{}/1.0.0/bin/{}", formula_name, formula_name))
        .unwrap();
    header.set_size(20);
    header.set_mode(0o755);
    header.set_cksum();

    let content = format!("#!/bin/sh\necho {}", formula_name);
    builder.append(&header, content.as_bytes()).unwrap();

    let tar_data = builder.into_inner().unwrap();

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar_data).unwrap();
    encoder.finish().unwrap()
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

async fn setup_mock_server(
    server: &MockServer,
    formulas: &[(&str, &[&str])], // (name, dependencies)
) -> Vec<(String, String)> {
    // Returns: Vec<(name, bottle_sha)>
    let mut results = Vec::new();

    for (name, deps) in formulas {
        let bottle = create_bottle_tarball(name);
        let bottle_sha = sha256_hex(&bottle);

        let deps_json: Vec<String> = deps.iter().map(|d| format!("\"{}\"", d)).collect();
        let deps_str = deps_json.join(", ");

        let formula_json = format!(
            r#"{{
                "name": "{}",
                "versions": {{ "stable": "1.0.0" }},
                "dependencies": [{}],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "arm64_sonoma": {{
                                "url": "{}/bottles/{}-1.0.0.arm64_sonoma.bottle.tar.gz",
                                "sha256": "{}"
                            }}
                        }}
                    }}
                }}
            }}"#,
            name,
            deps_str,
            server.uri(),
            name,
            bottle_sha
        );

        // Mount formula API
        Mock::given(method("GET"))
            .and(path(format!("/{}.json", name)))
            .respond_with(ResponseTemplate::new(200).set_body_string(&formula_json))
            .mount(server)
            .await;

        // Mount bottle download
        Mock::given(method("GET"))
            .and(path(format!(
                "/bottles/{}-1.0.0.arm64_sonoma.bottle.tar.gz",
                name
            )))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bottle))
            .mount(server)
            .await;

        results.push((name.to_string(), bottle_sha));
    }

    results
}

fn create_installer(
    root: &Path,
    prefix: &Path,
    api_base_url: &str,
) -> Result<Installer, zb_core::Error> {
    fs::create_dir_all(root.join("db")).unwrap();

    let api_client = ApiClient::with_base_url(api_base_url.to_string());
    let blob_cache = BlobCache::new(&root.join("cache")).map_err(|e| zb_core::Error::StoreCorruption {
        message: format!("failed to create blob cache: {e}"),
    })?;
    let store = Store::new(root).map_err(|e| zb_core::Error::StoreCorruption {
        message: format!("failed to create store: {e}"),
    })?;
    let cellar = Cellar::new(root).map_err(|e| zb_core::Error::StoreCorruption {
        message: format!("failed to create cellar: {e}"),
    })?;
    let linker = Linker::new(prefix).map_err(|e| zb_core::Error::StoreCorruption {
        message: format!("failed to create linker: {e}"),
    })?;
    let db = Database::open(&root.join("db/zb.sqlite3"))?;

    Ok(Installer::new(
        api_client, blob_cache, store, cellar, linker, db, 8, None,
    ))
}

async fn run_smoke_bench() -> Result<SmokeResult, zb_core::Error> {
    let mock_server = MockServer::start().await;
    let tmp = TempDir::new().unwrap();

    // Set up a dependency graph:
    // mainpkg -> libfoo -> libbase
    // mainpkg -> libbar -> libbase
    let formulas = [
        ("libbase", &[][..]),
        ("libfoo", &["libbase"][..]),
        ("libbar", &["libbase"][..]),
        ("mainpkg", &["libfoo", "libbar"][..]),
    ];

    setup_mock_server(&mock_server, &formulas).await;

    let root = tmp.path().join("zerobrew");
    let prefix = tmp.path().join("homebrew");

    let mut installer = create_installer(&root, &prefix, &mock_server.uri())?;

    let total_start = Instant::now();

    // Plan (resolve)
    let resolve_start = Instant::now();
    let plan = installer.plan("mainpkg").await?;
    let resolve_ms = resolve_start.elapsed().as_millis() as u64;
    let formulas_count = plan.formulas.len();

    // Execute (download + install)
    let download_start = Instant::now();
    installer.execute(plan, true).await?;
    let download_ms = download_start.elapsed().as_millis() as u64;

    // The install time is embedded in execute, but we can measure uninstall separately
    let install_ms = download_ms; // Combined for now

    // Uninstall
    let uninstall_start = Instant::now();
    installer.uninstall("mainpkg")?;
    installer.uninstall("libfoo")?;
    installer.uninstall("libbar")?;
    installer.uninstall("libbase")?;
    let uninstall_ms = uninstall_start.elapsed().as_millis() as u64;

    // GC
    installer.gc()?;

    let total_ms = total_start.elapsed().as_millis() as u64;

    Ok(SmokeResult {
        total_ms,
        resolve_ms,
        download_ms,
        install_ms,
        uninstall_ms,
        formulas_count,
    })
}

async fn run_real_bench(formula: &str) -> Result<BenchResult, Box<dyn std::error::Error>> {
    use std::process::Command;

    println!("Running real benchmark for '{}'...\n", formula);

    // Check if brew is available
    let brew_check = Command::new("brew").arg("--version").output();
    if brew_check.is_err() {
        return Err("brew not found in PATH".into());
    }

    // Check if zb is available
    let zb_check = Command::new("zb").arg("--version").output();
    if zb_check.is_err() {
        return Err("zb not found in PATH (run: cargo install --path zb_cli)".into());
    }

    // Ensure formula is not installed
    println!("Cleaning up any existing installations...");
    let _ = Command::new("brew")
        .args(["uninstall", "--ignore-dependencies", formula])
        .output();
    let _ = Command::new("zb").args(["uninstall", formula]).output();

    // Clean zerobrew caches for cold test
    let _ = Command::new("rm")
        .args(["-rf", "/opt/zerobrew/db", "/opt/zerobrew/cache", "/opt/zerobrew/store"])
        .output();

    // Run Homebrew install (cold)
    println!("Running: brew install {} (cold)...", formula);
    let brew_start = Instant::now();
    let brew_result = Command::new("brew")
        .args(["install", formula])
        .output()
        .expect("Failed to run brew");
    let brew_cold_ms = brew_start.elapsed().as_millis() as u64;

    if !brew_result.status.success() {
        let stderr = String::from_utf8_lossy(&brew_result.stderr);
        eprintln!("brew install failed: {}", stderr);
        return Err("brew install failed".into());
    }
    println!("  Homebrew cold install: {} ms", brew_cold_ms);

    // Uninstall for zerobrew test
    let _ = Command::new("brew")
        .args(["uninstall", "--ignore-dependencies", formula])
        .output();

    // Run zerobrew install (cold - no cache)
    println!("Running: zb install {} (cold)...", formula);
    let zb_cold_start = Instant::now();
    let zb_result = Command::new("zb")
        .args(["install", formula])
        .output()
        .expect("Failed to run zb");
    let zb_cold_ms = zb_cold_start.elapsed().as_millis() as u64;

    if !zb_result.status.success() {
        let stderr = String::from_utf8_lossy(&zb_result.stderr);
        eprintln!("zb install failed: {}", stderr);
        return Err("zb install failed".into());
    }
    println!("  Zerobrew cold install: {} ms", zb_cold_ms);

    // Uninstall for warm test
    let _ = Command::new("zb").args(["uninstall", formula]).output();

    // Run zerobrew install (warm - cached)
    println!("Running: zb install {} (warm)...", formula);
    let zb_warm_start = Instant::now();
    let zb_warm_result = Command::new("zb")
        .args(["install", formula])
        .output()
        .expect("Failed to run zb");
    let zb_warm_ms = zb_warm_start.elapsed().as_millis() as u64;

    if !zb_warm_result.status.success() {
        let stderr = String::from_utf8_lossy(&zb_warm_result.stderr);
        eprintln!("zb warm install failed: {}", stderr);
    }
    println!("  Zerobrew warm install: {} ms", zb_warm_ms);

    // Calculate speedup
    let cold_speedup = brew_cold_ms as f64 / zb_cold_ms as f64;
    let warm_speedup = brew_cold_ms as f64 / zb_warm_ms as f64;

    println!("\n=== Results ===");
    println!("Formula: {}", formula);
    println!("Homebrew cold:  {} ms", brew_cold_ms);
    println!("Zerobrew cold:  {} ms ({:.1}x faster)", zb_cold_ms, cold_speedup);
    println!("Zerobrew warm:  {} ms ({:.1}x faster)", zb_warm_ms, warm_speedup);

    Ok(BenchResult {
        name: formula.to_string(),
        cold_install_ms: zb_cold_ms,
        warm_reinstall_ms: zb_warm_ms,
        speedup: cold_speedup,
    })
}

// Abridged list for quick testing (fast packages with few deps)
const POPULAR_PACKAGES_ABRIDGED: &[&str] = &[
    "jq", "tree", "htop", "bat", "fd", "ripgrep", "fzf",
    "wget", "curl", "git", "tmux", "zoxide",
    "openssl@3", "sqlite", "readline", "pcre2", "zstd", "lz4",
    "node", "go", "ruby", "gh",
];

// Top 100 Homebrew packages from analytics (30d install count)
// Fetched from: https://formulae.brew.sh/api/analytics/install/30d.json
const POPULAR_PACKAGES: &[&str] = &[
    "ca-certificates", "openssl@3", "xz", "sqlite", "readline",
    "icu4c@78", "python@3.14", "awscli", "node", "harfbuzz",
    "ncurses", "gh", "pcre2", "libpng", "zstd",
    "glib", "lz4", "gettext", "libngtcp2", "libnghttp3",
    "pkgconf", "libunistring", "mpdecimal", "brotli", "jpeg-turbo",
    "xorgproto", "ffmpeg", "cmake", "libnghttp2", "go",
    "uv", "gmp", "libtiff", "fontconfig", "python@3.13",
    "git", "little-cms2", "dav1d", "openexr", "c-ares",
    "tesseract", "p11-kit", "imagemagick", "zlib", "libx11",
    "freetype", "protobuf", "gnupg", "openjph", "libtasn1",
    "ruby", "gnutls", "expat", "libsodium", "simdjson",
    "gemini-cli", "libarchive", "pyenv", "pixman", "curl",
    "opus", "unbound", "cairo", "pango", "leptonica",
    "libxcb", "jpeg-xl", "coreutils", "certifi", "krb5",
    "docker", "libheif", "webp", "libxext", "libxau",
    "gcc", "bzip2", "libxdmcp", "abseil", "xcbeautify",
    "libuv", "giflib", "utf8proc", "libxrender", "m4",
    "graphite2", "openjdk", "uvwasi", "libffi", "libdeflate",
    "llvm", "aom", "lzo", "libevent", "libgpg-error",
    "libidn2", "berkeley-db@5", "deno", "libedit", "oniguruma",
];

async fn run_suite_bench(count: usize, quick: bool) -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Command;

    let source = if quick { "abridged" } else { "top 100" };
    println!("Running benchmark suite ({} packages from {} list)...\n", count, source);

    let package_list = if quick { POPULAR_PACKAGES_ABRIDGED } else { POPULAR_PACKAGES };
    let packages: Vec<&str> = package_list.iter().take(count).copied().collect();
    let mut results: Vec<BenchResult> = Vec::new();
    let mut failures: Vec<(String, String)> = Vec::new();

    for (i, formula) in packages.iter().enumerate() {
        println!("[{}/{}] Benchmarking: {}", i + 1, count, formula);

        // Clean up
        let _ = Command::new("brew")
            .args(["uninstall", "--ignore-dependencies", formula])
            .output();
        let _ = Command::new("zb").args(["uninstall", formula]).output();
        let _ = Command::new("rm")
            .args(["-rf", "/opt/zerobrew/db"])
            .output();

        match run_real_bench(formula).await {
            Ok(result) => results.push(result),
            Err(e) => {
                println!("  FAILED: {}", e);
                failures.push((formula.to_string(), e.to_string()));
            }
        }
        println!();
    }

    // Summary
    println!("\n=== Suite Summary ===");
    println!("Tested: {} packages", count);
    println!("Passed: {}", results.len());
    println!("Failed: {}", failures.len());

    if !results.is_empty() {
        let avg_speedup: f64 = results.iter().map(|r| r.speedup).sum::<f64>() / results.len() as f64;
        let avg_warm_speedup: f64 = results.iter().map(|r| {
            if r.warm_reinstall_ms > 0 {
                r.cold_install_ms as f64 / r.warm_reinstall_ms as f64
            } else {
                1.0
            }
        }).sum::<f64>() / results.len() as f64;

        println!("\nPerformance:");
        println!("  Avg cold speedup vs Homebrew: {:.1}x", avg_speedup);
        println!("  Avg warm speedup vs cold:     {:.1}x", avg_warm_speedup);
    }

    if !failures.is_empty() {
        println!("\nFailed packages:");
        for (name, err) in &failures {
            println!("  {} - {}", name, err);
        }
    }

    // Clean up all installed packages
    println!("\nCleaning up...");
    for formula in &packages {
        let _ = Command::new("brew")
            .args(["uninstall", "--ignore-dependencies", formula])
            .output();
        let _ = Command::new("zb").args(["uninstall", formula]).output();
    }
    // Also uninstall all zb packages (in case of dependencies)
    let _ = Command::new("zb").args(["uninstall"]).output();
    println!("Done.");

    Ok(())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Smoke => {
            println!("Running smoke benchmark...\n");

            match run_smoke_bench().await {
                Ok(result) => {
                    println!("Smoke Benchmark Results");
                    println!("=======================");
                    println!("Total time:      {} ms", result.total_ms);
                    println!("Resolve time:    {} ms", result.resolve_ms);
                    println!("Download time:   {} ms", result.download_ms);
                    println!("Install time:    {} ms", result.install_ms);
                    println!("Uninstall time:  {} ms", result.uninstall_ms);
                    println!("Formulas count:  {}", result.formulas_count);
                    println!();

                    // Output JSON
                    let json = serde_json::to_string_pretty(&result).unwrap();
                    println!("JSON Output:\n{}", json);

                    // Check acceptance criteria
                    if result.total_ms < 60000 {
                        println!("\n[PASS] Total time < 60s");
                    } else {
                        println!("\n[FAIL] Total time >= 60s");
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Benchmark failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Real { formula } => {
            match run_real_bench(&formula).await {
                Ok(result) => {
                    let json = serde_json::to_string_pretty(&result).unwrap();
                    println!("\n{}", json);
                }
                Err(e) => {
                    eprintln!("Benchmark failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Suite { count, quick } => {
            if let Err(e) = run_suite_bench(count, quick).await {
                eprintln!("Suite benchmark failed: {}", e);
                std::process::exit(1);
            }
        }
    }
}
