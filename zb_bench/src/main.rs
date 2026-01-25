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
        api_client, blob_cache, store, cellar, linker, db, 8,
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
    // For real benchmarks, we'd use the actual Homebrew API
    // For now, this is a placeholder that shows what the output would look like

    println!("Running real benchmark for '{}'...", formula);
    println!("Note: This requires a working Homebrew installation for comparison.");

    // Check if brew is available
    let brew_check = std::process::Command::new("brew")
        .arg("--version")
        .output();

    if brew_check.is_err() {
        return Err("brew not found in PATH".into());
    }

    // For now, return placeholder data
    // A real implementation would:
    // 1. Run `brew install <formula>` and time it
    // 2. Run `brew uninstall <formula>`
    // 3. Run `zb install <formula>` and time it
    // 4. Compare times

    println!("Real benchmarking not yet implemented.");
    println!("Use 'zb bench smoke' for mocked benchmarks.");

    Ok(BenchResult {
        name: formula.to_string(),
        cold_install_ms: 0,
        warm_reinstall_ms: 0,
        speedup: 0.0,
    })
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
                    println!("{}", json);
                }
                Err(e) => {
                    eprintln!("Benchmark failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
