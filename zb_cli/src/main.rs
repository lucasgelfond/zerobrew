use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Instant;

use zb_io::install::create_installer;

#[derive(Parser)]
#[command(name = "zb")]
#[command(about = "Zerobrew - A fast Homebrew-compatible package installer")]
#[command(version)]
struct Cli {
    /// Root directory for zerobrew data
    #[arg(long, default_value = "/opt/zerobrew")]
    root: PathBuf,

    /// Prefix directory for linked binaries
    #[arg(long, default_value = "/opt/homebrew")]
    prefix: PathBuf,

    /// Number of parallel downloads
    #[arg(long, default_value = "8")]
    concurrency: usize,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install a formula
    Install {
        /// Formula name to install
        formula: String,

        /// Skip linking executables
        #[arg(long)]
        no_link: bool,
    },

    /// Uninstall a formula
    Uninstall {
        /// Formula name to uninstall
        formula: String,
    },

    /// List installed formulas
    List,

    /// Show info about an installed formula
    Info {
        /// Formula name
        formula: String,
    },

    /// Garbage collect unreferenced store entries
    Gc,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), zb_core::Error> {
    let mut installer = create_installer(&cli.root, &cli.prefix, cli.concurrency)?;

    match cli.command {
        Commands::Install { formula, no_link } => {
            let start = Instant::now();
            println!("==> Installing {}...", formula);

            let plan = installer.plan(&formula).await?;

            println!("==> Resolving dependencies...");
            for f in &plan.formulas {
                println!("    {} {}", f.name, f.versions.stable);
            }

            println!("==> Downloading bottles...");
            installer.execute(plan, !no_link).await?;

            let elapsed = start.elapsed();
            println!(
                "==> Installed {} in {:.2}s",
                formula,
                elapsed.as_secs_f64()
            );
        }

        Commands::Uninstall { formula } => {
            println!("==> Uninstalling {}...", formula);
            installer.uninstall(&formula)?;
            println!("==> Uninstalled {}", formula);
        }

        Commands::List => {
            let installed = installer.list_installed()?;

            if installed.is_empty() {
                println!("No formulas installed.");
            } else {
                for keg in installed {
                    println!("{} {}", keg.name, keg.version);
                }
            }
        }

        Commands::Info { formula } => {
            if let Some(keg) = installer.get_installed(&formula) {
                println!("Name:       {}", keg.name);
                println!("Version:    {}", keg.version);
                println!("Store key:  {}", keg.store_key);
                println!(
                    "Installed:  {}",
                    chrono_lite_format(keg.installed_at)
                );
            } else {
                println!("Formula '{}' is not installed.", formula);
            }
        }

        Commands::Gc => {
            println!("==> Running garbage collection...");
            let removed = installer.gc()?;

            if removed.is_empty() {
                println!("No unreferenced store entries to remove.");
            } else {
                for key in &removed {
                    println!("    Removed {}", &key[..12]);
                }
                println!("==> Removed {} store entries", removed.len());
            }
        }
    }

    Ok(())
}

fn chrono_lite_format(timestamp: i64) -> String {
    // Simple timestamp formatting without pulling in chrono
    use std::time::{Duration, UNIX_EPOCH};

    let dt = UNIX_EPOCH + Duration::from_secs(timestamp as u64);
    format!("{:?}", dt)
}
