use clap::{Parser, Subcommand};
use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use zb_brewfile::{BrewfileParser, Exporter, Importer};
use zb_io::install::create_installer;
use zb_io::{Database, InstallProgress, ProgressCallback};
use zb_migrate::{IncompatibleReason, MigrationPlan, Migrator};
use zb_services::{ServiceManager, ServiceState};

#[derive(Parser)]
#[command(name = "zb")]
#[command(about = "Zerobrew - A fast Homebrew-compatible package installer")]
#[command(version)]
struct Cli {
    /// Root directory for zerobrew data
    #[arg(long, default_value = "/opt/zerobrew")]
    root: PathBuf,

    /// Prefix directory for linked binaries
    #[arg(long, default_value = "/opt/zerobrew/prefix")]
    prefix: PathBuf,

    /// Number of parallel downloads
    #[arg(long, default_value = "48")]
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

    /// Uninstall a formula (or all formulas if no name given)
    Uninstall {
        /// Formula name to uninstall (omit to uninstall all)
        formula: Option<String>,
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

    /// Reset zerobrew (delete all data for cold install testing)
    Reset {
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Initialize zerobrew directories with correct permissions
    Init,

    /// Migrate packages from Homebrew to Zerobrew
    Migrate {
        /// Specific formulas to migrate (default: all user-requested)
        formulas: Vec<String>,

        /// Only show what would be migrated (don't actually install)
        #[arg(long)]
        dry_run: bool,

        /// Homebrew prefix path (default: /opt/homebrew)
        #[arg(long, default_value = "/opt/homebrew")]
        homebrew_prefix: PathBuf,
    },

    /// Manage services
    #[command(subcommand)]
    Services(ServicesCommands),

    /// Import packages from a Brewfile
    Import {
        /// Path to Brewfile (default: ./Brewfile)
        #[arg(default_value = "./Brewfile")]
        brewfile: PathBuf,

        /// Show what would be installed without installing
        #[arg(long)]
        dry_run: bool,

        /// Enable services with restart_service hints
        #[arg(long)]
        with_services: bool,
    },

    /// Export installed packages to Brewfile format
    Export {
        /// Output path (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ServicesCommands {
    /// Start a service
    Start {
        /// Formula name
        formula: String,
    },

    /// Stop a service
    Stop {
        /// Formula name
        formula: String,
    },

    /// Restart a service
    Restart {
        /// Formula name
        formula: String,
    },

    /// Show service status
    Status {
        /// Formula name
        formula: String,

        /// JSON output
        #[arg(long)]
        json: bool,
    },

    /// List all services
    List {
        /// JSON output
        #[arg(long)]
        json: bool,
    },

    /// Enable auto-start at login
    Enable {
        /// Formula name
        formula: String,
    },

    /// Disable auto-start
    Disable {
        /// Formula name
        formula: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        eprintln!("{} {}", style("error:").red().bold(), e);
        std::process::exit(1);
    }
}

/// Check if zerobrew directories need initialization
fn needs_init(root: &Path, prefix: &Path) -> bool {
    // Check if directories exist and are writable
    let root_ok = root.exists() && is_writable(root);
    let prefix_ok = prefix.exists() && is_writable(prefix);
    !(root_ok && prefix_ok)
}

fn is_writable(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    // Try to check if we can write to this directory
    let test_file = path.join(".zb_write_test");
    match std::fs::write(&test_file, b"test") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            true
        }
        Err(_) => false,
    }
}

/// Run initialization - create directories and set permissions
fn run_init(root: &Path, prefix: &Path) -> Result<(), String> {
    println!("{} Initializing zerobrew...", style("==>").cyan().bold());

    let dirs_to_create: Vec<PathBuf> = vec![
        root.to_path_buf(),
        root.join("store"),
        root.join("db"),
        root.join("cache"),
        root.join("locks"),
        prefix.to_path_buf(),
        prefix.join("bin"),
        prefix.join("Cellar"),
    ];

    // Check if we need sudo
    let need_sudo = dirs_to_create.iter().any(|d| {
        if d.exists() {
            !is_writable(d)
        } else {
            // Check parent
            d.parent()
                .map(|p| p.exists() && !is_writable(p))
                .unwrap_or(true)
        }
    });

    if need_sudo {
        println!(
            "{}",
            style("    Creating directories (requires sudo)...").dim()
        );

        // Create directories with sudo
        for dir in &dirs_to_create {
            let status = Command::new("sudo")
                .args(["mkdir", "-p", &dir.to_string_lossy()])
                .status()
                .map_err(|e| format!("Failed to run sudo mkdir: {}", e))?;

            if !status.success() {
                return Err(format!("Failed to create directory: {}", dir.display()));
            }
        }

        // Change ownership to current user - use whoami for reliability
        let user = Command::new("whoami")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "root".to_string()));

        let status = Command::new("sudo")
            .args(["chown", "-R", &user, &root.to_string_lossy()])
            .status()
            .map_err(|e| format!("Failed to run sudo chown: {}", e))?;

        if !status.success() {
            return Err(format!("Failed to set ownership on {}", root.display()));
        }

        let status = Command::new("sudo")
            .args(["chown", "-R", &user, &prefix.to_string_lossy()])
            .status()
            .map_err(|e| format!("Failed to run sudo chown: {}", e))?;

        if !status.success() {
            return Err(format!("Failed to set ownership on {}", prefix.display()));
        }
    } else {
        // Create directories without sudo
        for dir in &dirs_to_create {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("Failed to create {}: {}", dir.display(), e))?;
        }
    }

    // Add to shell config if not already there
    add_to_path(prefix)?;

    println!("{} Initialization complete!", style("==>").cyan().bold());

    Ok(())
}

fn add_to_path(prefix: &Path) -> Result<(), String> {
    let shell = std::env::var("SHELL").unwrap_or_default();
    let home = std::env::var("HOME").map_err(|_| "HOME not set")?;

    let config_file = if shell.contains("zsh") {
        format!("{}/.zshrc", home)
    } else if shell.contains("bash") {
        let bash_profile = format!("{}/.bash_profile", home);
        if std::path::Path::new(&bash_profile).exists() {
            bash_profile
        } else {
            format!("{}/.bashrc", home)
        }
    } else {
        format!("{}/.profile", home)
    };

    let bin_path = prefix.join("bin");
    let path_export = format!("export PATH=\"{}:$PATH\"", bin_path.display());

    // Check if already in config
    let already_added = if let Ok(contents) = std::fs::read_to_string(&config_file) {
        contents.contains(&bin_path.to_string_lossy().to_string())
    } else {
        false
    };

    if !already_added {
        // Append to config
        let addition = format!("\n# zerobrew\n{}\n", path_export);
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config_file)
            .and_then(|mut f| {
                use std::io::Write;
                f.write_all(addition.as_bytes())
            })
            .map_err(|e| format!("Failed to update {}: {}", config_file, e))?;

        println!(
            "    {} Added {} to PATH in {}",
            style("✓").green(),
            bin_path.display(),
            config_file
        );
    }

    // Always check if PATH is actually set in current shell
    let current_path = std::env::var("PATH").unwrap_or_default();
    if !current_path.contains(&bin_path.to_string_lossy().to_string()) {
        println!(
            "    {} Run {} or restart your terminal",
            style("→").cyan(),
            style(format!("source {}", config_file)).cyan()
        );
    }

    Ok(())
}

/// Ensure zerobrew is initialized, prompting user if needed
fn ensure_init(root: &Path, prefix: &Path) -> Result<(), zb_core::Error> {
    if !needs_init(root, prefix) {
        return Ok(());
    }

    println!(
        "{} Zerobrew needs to be initialized first.",
        style("Note:").yellow().bold()
    );
    println!("    This will create directories at:");
    println!("      • {}", root.display());
    println!("      • {}", prefix.display());
    println!();

    print!("Initialize now? [Y/n] ");
    use std::io::{self, Write};
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let input = input.trim();

    if !input.is_empty() && !input.eq_ignore_ascii_case("y") && !input.eq_ignore_ascii_case("yes") {
        return Err(zb_core::Error::StoreCorruption {
            message: "Initialization required. Run 'zb init' first.".to_string(),
        });
    }

    run_init(root, prefix).map_err(|e| zb_core::Error::StoreCorruption { message: e })
}

fn suggest_homebrew(formula: &str, error: &zb_core::Error) {
    eprintln!();
    eprintln!(
        "{} This package can't be installed with zerobrew.",
        style("Note:").yellow().bold()
    );
    eprintln!("      Error: {}", error);
    eprintln!();
    eprintln!("      Try installing with Homebrew instead:");
    eprintln!(
        "      {}",
        style(format!("brew install {}", formula)).cyan()
    );
    eprintln!();
}

fn print_migration_plan(plan: &MigrationPlan) {
    println!();
    println!("{} Migration Plan", style("==>").cyan().bold());
    println!();

    if !plan.to_install.is_empty() {
        println!(
            "  {} ({}):",
            style("Packages to install").green().bold(),
            plan.to_install.len()
        );
        for name in &plan.to_install {
            println!("    {} {}", style("•").dim(), name);
        }
        println!();
    }

    if !plan.dependencies.is_empty() {
        println!(
            "  {} ({}):",
            style("Dependencies (auto-installed)").dim(),
            plan.dependencies.len()
        );
        for name in &plan.dependencies {
            println!("    {} {}", style("•").dim(), style(name).dim());
        }
        println!();
    }

    if !plan.already_installed.is_empty() {
        println!(
            "  {} ({}):",
            style("Already installed in Zerobrew").yellow(),
            plan.already_installed.len()
        );
        for name in &plan.already_installed {
            println!("    {} {}", style("✓").green(), name);
        }
        println!();
    }

    if !plan.incompatible.is_empty() {
        println!(
            "  {} ({}):",
            style("Cannot migrate").red(),
            plan.incompatible.len()
        );
        for item in &plan.incompatible {
            let reason = match &item.reason {
                IncompatibleReason::RequiresTap(tap) => format!("requires tap: {}", tap),
                IncompatibleReason::AlreadyInstalled => "already installed".to_string(),
                IncompatibleReason::ApiError(e) => format!("API error: {}", e),
            };
            println!(
                "    {} {} ({})",
                style("✗").red(),
                item.name,
                style(reason).dim()
            );
        }
        println!();
    }

    if !plan.services_warning.is_empty() {
        println!(
            "  {} ({}):",
            style("⚠ Services running (manage separately)").yellow(),
            plan.services_warning.len()
        );
        for name in &plan.services_warning {
            println!("    {} {}", style("⚠").yellow(), name);
        }
        println!();
    }
}

async fn run(cli: Cli) -> Result<(), zb_core::Error> {
    // Handle init separately - it doesn't need the installer
    if matches!(cli.command, Commands::Init) {
        return run_init(&cli.root, &cli.prefix)
            .map_err(|e| zb_core::Error::StoreCorruption { message: e });
    }

    // For reset, handle specially since directories may not be writable
    if matches!(cli.command, Commands::Reset { .. }) {
        // Skip init check for reset
    } else {
        // Ensure initialized before other commands
        ensure_init(&cli.root, &cli.prefix)?;
    }

    let mut installer = create_installer(&cli.root, &cli.prefix, cli.concurrency)?;

    match cli.command {
        Commands::Init => unreachable!(), // Handled above
        Commands::Install { formula, no_link } => {
            let start = Instant::now();
            println!(
                "{} Installing {}...",
                style("==>").cyan().bold(),
                style(&formula).bold()
            );

            let plan = match installer.plan(&formula).await {
                Ok(p) => p,
                Err(e) => {
                    suggest_homebrew(&formula, &e);
                    return Err(e);
                }
            };

            println!(
                "{} Resolving dependencies ({} packages)...",
                style("==>").cyan().bold(),
                plan.formulas.len()
            );
            for f in &plan.formulas {
                println!(
                    "    {} {}",
                    style(&f.name).green(),
                    style(&f.versions.stable).dim()
                );
            }

            // Set up progress display
            let multi = MultiProgress::new();
            let bars: Arc<Mutex<HashMap<String, ProgressBar>>> =
                Arc::new(Mutex::new(HashMap::new()));

            let download_style = ProgressStyle::default_bar()
                .template(
                    "    {prefix:<16} {bar:25.cyan/dim} {bytes:>10}/{total_bytes:<10} {eta:>6}",
                )
                .unwrap()
                .progress_chars("━━╸");

            let spinner_style = ProgressStyle::default_spinner()
                .template("    {prefix:<16} {spinner:.cyan} {msg}")
                .unwrap()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏");

            let done_style = ProgressStyle::default_spinner()
                .template("    {prefix:<16} {msg}")
                .unwrap();

            println!(
                "{} Downloading and installing...",
                style("==>").cyan().bold()
            );

            let bars_clone = bars.clone();
            let multi_clone = multi.clone();
            let download_style_clone = download_style.clone();
            let spinner_style_clone = spinner_style.clone();
            let done_style_clone = done_style.clone();

            let progress_callback: Arc<ProgressCallback> = Arc::new(Box::new(move |event| {
                let mut bars = bars_clone.lock().unwrap();
                match event {
                    InstallProgress::DownloadStarted { name, total_bytes } => {
                        let pb = if let Some(total) = total_bytes {
                            let pb = multi_clone.add(ProgressBar::new(total));
                            pb.set_style(download_style_clone.clone());
                            pb
                        } else {
                            let pb = multi_clone.add(ProgressBar::new_spinner());
                            pb.set_style(spinner_style_clone.clone());
                            pb.set_message("downloading...");
                            pb.enable_steady_tick(std::time::Duration::from_millis(80));
                            pb
                        };
                        pb.set_prefix(name.clone());
                        bars.insert(name, pb);
                    }
                    InstallProgress::DownloadProgress {
                        name,
                        downloaded,
                        total_bytes,
                    } => {
                        if let Some(pb) = bars.get(&name)
                            && total_bytes.is_some()
                        {
                            pb.set_position(downloaded);
                        }
                    }
                    InstallProgress::DownloadCompleted { name, total_bytes } => {
                        if let Some(pb) = bars.get(&name) {
                            if total_bytes > 0 {
                                pb.set_position(total_bytes);
                            }
                            pb.set_style(spinner_style_clone.clone());
                            pb.set_message("unpacking...");
                            pb.enable_steady_tick(std::time::Duration::from_millis(80));
                        }
                    }
                    InstallProgress::UnpackStarted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_message("unpacking...");
                        }
                    }
                    InstallProgress::UnpackCompleted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_message("linking...");
                        }
                    }
                    InstallProgress::LinkStarted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_message("linking...");
                        }
                    }
                    InstallProgress::LinkCompleted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_style(done_style_clone.clone());
                            pb.set_message(format!("{} installed", style("✓").green()));
                            pb.finish();
                        }
                    }
                }
            }));

            let result = match installer
                .execute_with_progress(plan, !no_link, Some(progress_callback))
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    suggest_homebrew(&formula, &e);
                    return Err(e);
                }
            };

            // Finish any remaining bars
            {
                let bars = bars.lock().unwrap();
                for (_, pb) in bars.iter() {
                    if !pb.is_finished() {
                        pb.finish();
                    }
                }
            }

            let elapsed = start.elapsed();
            println!();
            println!(
                "{} Installed {} packages in {:.2}s",
                style("==>").cyan().bold(),
                style(result.installed).green().bold(),
                elapsed.as_secs_f64()
            );
        }

        Commands::Uninstall { formula } => match formula {
            Some(name) => {
                println!(
                    "{} Uninstalling {}...",
                    style("==>").cyan().bold(),
                    style(&name).bold()
                );
                installer.uninstall(&name)?;
                println!(
                    "{} Uninstalled {}",
                    style("==>").cyan().bold(),
                    style(&name).green()
                );
            }
            None => {
                let installed = installer.list_installed()?;
                if installed.is_empty() {
                    println!("No formulas installed.");
                    return Ok(());
                }

                println!(
                    "{} Uninstalling {} packages...",
                    style("==>").cyan().bold(),
                    installed.len()
                );

                for keg in installed {
                    print!("    {} {}...", style("○").dim(), keg.name);
                    installer.uninstall(&keg.name)?;
                    println!(" {}", style("✓").green());
                }

                println!("{} Uninstalled all packages", style("==>").cyan().bold());
            }
        },

        Commands::List => {
            let installed = installer.list_installed()?;

            if installed.is_empty() {
                println!("No formulas installed.");
            } else {
                for keg in installed {
                    println!("{} {}", style(&keg.name).bold(), style(&keg.version).dim());
                }
            }
        }

        Commands::Info { formula } => {
            if let Some(keg) = installer.get_installed(&formula) {
                println!("{}       {}", style("Name:").dim(), style(&keg.name).bold());
                println!("{}    {}", style("Version:").dim(), keg.version);
                println!("{}  {}", style("Store key:").dim(), &keg.store_key[..12]);
                println!(
                    "{}  {}",
                    style("Installed:").dim(),
                    chrono_lite_format(keg.installed_at)
                );
            } else {
                println!("Formula '{}' is not installed.", formula);
            }
        }

        Commands::Gc => {
            println!(
                "{} Running garbage collection...",
                style("==>").cyan().bold()
            );
            let removed = installer.gc()?;

            if removed.is_empty() {
                println!("No unreferenced store entries to remove.");
            } else {
                for key in &removed {
                    println!("    {} Removed {}", style("✓").green(), &key[..12]);
                }
                println!(
                    "{} Removed {} store entries",
                    style("==>").cyan().bold(),
                    style(removed.len()).green().bold()
                );
            }
        }

        Commands::Reset { yes } => {
            if !cli.root.exists() && !cli.prefix.exists() {
                println!("Nothing to reset - directories do not exist.");
                return Ok(());
            }

            if !yes {
                println!(
                    "{} This will delete all zerobrew data at:",
                    style("Warning:").yellow().bold()
                );
                println!("      • {}", cli.root.display());
                println!("      • {}", cli.prefix.display());
                print!("Continue? [y/N] ");
                use std::io::{self, Write};
                io::stdout().flush().unwrap();

                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            // Remove directories - try without sudo first, then with
            for dir in [&cli.root, &cli.prefix] {
                if !dir.exists() {
                    continue;
                }

                println!(
                    "{} Removing {}...",
                    style("==>").cyan().bold(),
                    dir.display()
                );

                if std::fs::remove_dir_all(dir).is_err() {
                    // Try with sudo
                    let status = Command::new("sudo")
                        .args(["rm", "-rf", &dir.to_string_lossy()])
                        .status();

                    if status.is_err() || !status.unwrap().success() {
                        eprintln!(
                            "{} Failed to remove {}",
                            style("error:").red().bold(),
                            dir.display()
                        );
                        std::process::exit(1);
                    }
                }
            }

            // Re-initialize with correct permissions
            run_init(&cli.root, &cli.prefix)
                .map_err(|e| zb_core::Error::StoreCorruption { message: e })?;

            println!(
                "{} Reset complete. Ready for cold install.",
                style("==>").cyan().bold()
            );
        }

        Commands::Migrate {
            formulas,
            dry_run,
            homebrew_prefix,
        } => {
            let start = Instant::now();

            println!(
                "{} Scanning Homebrew at {}...",
                style("==>").cyan().bold(),
                homebrew_prefix.display()
            );

            let mut migrator = Migrator::with_prefix(&mut installer, &homebrew_prefix);

            if !migrator.is_homebrew_installed() {
                return Err(zb_core::Error::StoreCorruption {
                    message: format!(
                        "Homebrew not found at {}. Is Homebrew installed?",
                        homebrew_prefix.display()
                    ),
                });
            }

            // Create migration plan
            let specific = if formulas.is_empty() {
                None
            } else {
                Some(formulas.as_slice())
            };

            let plan = migrator
                .plan(specific)
                .map_err(|e| zb_core::Error::StoreCorruption {
                    message: format!("Failed to create migration plan: {}", e),
                })?;

            print_migration_plan(&plan);

            if plan.is_empty() {
                println!("{} Nothing to migrate.", style("==>").cyan().bold());
                return Ok(());
            }

            if dry_run {
                println!(
                    "{} Dry run - no packages were installed.",
                    style("==>").cyan().bold()
                );
                println!(
                    "    Run {} to actually migrate {} packages.",
                    style("zb migrate").cyan(),
                    plan.to_install.len()
                );
                return Ok(());
            }

            // Execute migration
            println!(
                "{} Migrating {} packages from Homebrew...",
                style("==>").cyan().bold(),
                plan.to_install.len()
            );

            // Set up progress display (same as install)
            let multi = MultiProgress::new();
            let bars: Arc<Mutex<HashMap<String, ProgressBar>>> =
                Arc::new(Mutex::new(HashMap::new()));

            let download_style = ProgressStyle::default_bar()
                .template(
                    "    {prefix:<16} {bar:25.cyan/dim} {bytes:>10}/{total_bytes:<10} {eta:>6}",
                )
                .unwrap()
                .progress_chars("━━╸");

            let spinner_style = ProgressStyle::default_spinner()
                .template("    {prefix:<16} {spinner:.cyan} {msg}")
                .unwrap()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏");

            let done_style = ProgressStyle::default_spinner()
                .template("    {prefix:<16} {msg}")
                .unwrap();

            let bars_clone = bars.clone();
            let multi_clone = multi.clone();
            let download_style_clone = download_style.clone();
            let spinner_style_clone = spinner_style.clone();
            let done_style_clone = done_style.clone();

            let progress_callback: Arc<ProgressCallback> = Arc::new(Box::new(move |event| {
                let mut bars = bars_clone.lock().unwrap();
                match event {
                    InstallProgress::DownloadStarted { name, total_bytes } => {
                        let pb = if let Some(total) = total_bytes {
                            let pb = multi_clone.add(ProgressBar::new(total));
                            pb.set_style(download_style_clone.clone());
                            pb
                        } else {
                            let pb = multi_clone.add(ProgressBar::new_spinner());
                            pb.set_style(spinner_style_clone.clone());
                            pb.set_message("downloading...");
                            pb.enable_steady_tick(std::time::Duration::from_millis(80));
                            pb
                        };
                        pb.set_prefix(name.clone());
                        bars.insert(name, pb);
                    }
                    InstallProgress::DownloadProgress {
                        name,
                        downloaded,
                        total_bytes,
                    } => {
                        if let Some(pb) = bars.get(&name)
                            && total_bytes.is_some()
                        {
                            pb.set_position(downloaded);
                        }
                    }
                    InstallProgress::DownloadCompleted { name, total_bytes } => {
                        if let Some(pb) = bars.get(&name) {
                            if total_bytes > 0 {
                                pb.set_position(total_bytes);
                            }
                            pb.set_style(spinner_style_clone.clone());
                            pb.set_message("unpacking...");
                            pb.enable_steady_tick(std::time::Duration::from_millis(80));
                        }
                    }
                    InstallProgress::UnpackStarted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_message("unpacking...");
                        }
                    }
                    InstallProgress::UnpackCompleted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_message("linking...");
                        }
                    }
                    InstallProgress::LinkStarted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_message("linking...");
                        }
                    }
                    InstallProgress::LinkCompleted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_style(done_style_clone.clone());
                            pb.set_message(format!("{} installed", style("✓").green()));
                            pb.finish();
                        }
                    }
                }
            }));

            let result = migrator
                .execute_with_progress(&plan, Some(progress_callback))
                .await
                .map_err(|e| zb_core::Error::StoreCorruption {
                    message: format!("Migration failed: {}", e),
                })?;

            // Finish any remaining bars
            {
                let bars = bars.lock().unwrap();
                for (_, pb) in bars.iter() {
                    if !pb.is_finished() {
                        pb.finish();
                    }
                }
            }

            let elapsed = start.elapsed();
            println!();
            println!("{} Migration complete!", style("==>").cyan().bold());
            println!(
                "    {} Installed: {} packages",
                style("✓").green(),
                style(result.installed.len()).green().bold()
            );

            if !result.failed.is_empty() {
                println!(
                    "    {} Failed: {} packages",
                    style("✗").red(),
                    style(result.failed.len()).red().bold()
                );
                for (name, err) in &result.failed {
                    println!("      {} {}: {}", style("•").dim(), name, style(err).dim());
                }
            }

            println!("    Time: {:.2}s", elapsed.as_secs_f64());
            println!();
            println!(
                "    {} Your Homebrew packages are still installed.",
                style("Note:").yellow()
            );
            println!(
                "    To clean up: {}",
                style("brew uninstall --force $(brew list --formula)").cyan()
            );
        }

        Commands::Services(services_cmd) => {
            let db_path = cli.root.join("db/zb.sqlite3");
            let db = Database::open(&db_path)?;

            let mut service_manager = ServiceManager::new(&cli.prefix, db).map_err(|e| {
                zb_core::Error::StoreCorruption {
                    message: format!("Failed to create service manager: {}", e),
                }
            })?;

            match services_cmd {
                ServicesCommands::Start { formula } => {
                    println!(
                        "{} Starting {}...",
                        style("==>").cyan().bold(),
                        style(&formula).bold()
                    );

                    service_manager.start(&formula).await.map_err(|e| {
                        zb_core::Error::StoreCorruption {
                            message: format!("Failed to start service: {}", e),
                        }
                    })?;

                    println!(
                        "{} Started {}",
                        style("==>").cyan().bold(),
                        style(&formula).green()
                    );
                }

                ServicesCommands::Stop { formula } => {
                    println!(
                        "{} Stopping {}...",
                        style("==>").cyan().bold(),
                        style(&formula).bold()
                    );

                    service_manager.stop(&formula).map_err(|e| {
                        zb_core::Error::StoreCorruption {
                            message: format!("Failed to stop service: {}", e),
                        }
                    })?;

                    println!(
                        "{} Stopped {}",
                        style("==>").cyan().bold(),
                        style(&formula).green()
                    );
                }

                ServicesCommands::Restart { formula } => {
                    println!(
                        "{} Restarting {}...",
                        style("==>").cyan().bold(),
                        style(&formula).bold()
                    );

                    service_manager.restart(&formula).await.map_err(|e| {
                        zb_core::Error::StoreCorruption {
                            message: format!("Failed to restart service: {}", e),
                        }
                    })?;

                    println!(
                        "{} Restarted {}",
                        style("==>").cyan().bold(),
                        style(&formula).green()
                    );
                }

                ServicesCommands::Status { formula, json } => {
                    let status = service_manager.status(&formula).map_err(|e| {
                        zb_core::Error::StoreCorruption {
                            message: format!("Failed to get status: {}", e),
                        }
                    })?;

                    if json {
                        let json_output = serde_json::json!({
                            "name": status.name,
                            "state": format!("{:?}", status.state),
                            "pid": status.pid,
                            "exit_code": status.exit_code,
                            "enabled": status.enabled,
                        });
                        println!("{}", serde_json::to_string_pretty(&json_output).unwrap());
                    } else {
                        println!("{} {}", style("Name:").dim(), style(&status.name).bold());

                        let state_str = match status.state {
                            ServiceState::Running => style("running").green(),
                            ServiceState::Stopped => style("stopped").dim(),
                            ServiceState::Failed => style("failed").red(),
                            ServiceState::Scheduled => style("scheduled").yellow(),
                            ServiceState::Unknown => style("unknown").dim(),
                        };
                        println!("{} {}", style("State:").dim(), state_str);

                        if let Some(pid) = status.pid {
                            println!("{} {}", style("PID:").dim(), pid);
                        }

                        if let Some(exit_code) = status.exit_code {
                            println!("{} {}", style("Exit code:").dim(), exit_code);
                        }

                        let enabled_str = if status.enabled {
                            style("yes").green()
                        } else {
                            style("no").dim()
                        };
                        println!("{} {}", style("Enabled:").dim(), enabled_str);
                    }
                }

                ServicesCommands::List { json } => {
                    let services =
                        service_manager
                            .list()
                            .map_err(|e| zb_core::Error::StoreCorruption {
                                message: format!("Failed to list services: {}", e),
                            })?;

                    if services.is_empty() {
                        println!("No services found.");
                        return Ok(());
                    }

                    if json {
                        let json_output: Vec<_> = services
                            .iter()
                            .map(|s| {
                                serde_json::json!({
                                    "name": s.name,
                                    "state": format!("{:?}", s.state),
                                    "pid": s.pid,
                                    "enabled": s.enabled,
                                })
                            })
                            .collect();
                        println!("{}", serde_json::to_string_pretty(&json_output).unwrap());
                    } else {
                        println!(
                            "{:<20} {:<12} {:<8} {:<8}",
                            style("Name").bold(),
                            style("State").bold(),
                            style("PID").bold(),
                            style("Enabled").bold()
                        );

                        for svc in services {
                            let state_str = match svc.state {
                                ServiceState::Running => style("running").green().to_string(),
                                ServiceState::Stopped => style("stopped").dim().to_string(),
                                ServiceState::Failed => style("failed").red().to_string(),
                                ServiceState::Scheduled => style("scheduled").yellow().to_string(),
                                ServiceState::Unknown => style("unknown").dim().to_string(),
                            };

                            let pid_str = svc
                                .pid
                                .map(|p| p.to_string())
                                .unwrap_or_else(|| "-".to_string());

                            let enabled_str = if svc.enabled {
                                style("yes").green().to_string()
                            } else {
                                style("no").dim().to_string()
                            };

                            println!(
                                "{:<20} {:<12} {:<8} {:<8}",
                                svc.name, state_str, pid_str, enabled_str
                            );
                        }
                    }
                }

                ServicesCommands::Enable { formula } => {
                    println!(
                        "{} Enabling {}...",
                        style("==>").cyan().bold(),
                        style(&formula).bold()
                    );

                    service_manager.enable(&formula).await.map_err(|e| {
                        zb_core::Error::StoreCorruption {
                            message: format!("Failed to enable service: {}", e),
                        }
                    })?;

                    println!(
                        "{} Enabled {} (will start at login)",
                        style("==>").cyan().bold(),
                        style(&formula).green()
                    );
                }

                ServicesCommands::Disable { formula } => {
                    println!(
                        "{} Disabling {}...",
                        style("==>").cyan().bold(),
                        style(&formula).bold()
                    );

                    service_manager.disable(&formula).map_err(|e| {
                        zb_core::Error::StoreCorruption {
                            message: format!("Failed to disable service: {}", e),
                        }
                    })?;

                    println!(
                        "{} Disabled {} (will not start at login)",
                        style("==>").cyan().bold(),
                        style(&formula).green()
                    );
                }
            }
        }

        Commands::Import {
            brewfile,
            dry_run,
            with_services,
        } => {
            println!(
                "{} Parsing Brewfile: {}...",
                style("==>").cyan().bold(),
                brewfile.display()
            );

            let content = std::fs::read_to_string(&brewfile).map_err(|e| {
                zb_core::Error::StoreCorruption {
                    message: format!("Failed to read Brewfile: {}", e),
                }
            })?;

            let parsed =
                BrewfileParser::parse(&content).map_err(|e| zb_core::Error::StoreCorruption {
                    message: format!("Failed to parse Brewfile: {}", e),
                })?;

            println!("    Found {} entries", parsed.entries.len());
            println!();

            // Create service manager if with_services flag is set
            let mut service_manager_opt = if with_services {
                let db_path = cli.root.join("db/zb.sqlite3");
                let db = Database::open(&db_path)?;
                Some(ServiceManager::new(&cli.prefix, db).map_err(|e| {
                    zb_core::Error::StoreCorruption {
                        message: format!("Failed to create service manager: {}", e),
                    }
                })?)
            } else {
                None
            };

            let mut importer = if let Some(ref mut sm) = service_manager_opt {
                Importer::with_services(&mut installer, sm)
            } else {
                Importer::new(&mut installer)
            };

            let plan = importer.plan(&parsed);

            // Print plan
            println!("{} Import Plan", style("==>").cyan().bold());
            println!();

            if !plan.to_install.is_empty() {
                println!(
                    "  {} ({}):",
                    style("Packages to install").green().bold(),
                    plan.to_install.len()
                );
                for entry in &plan.to_install {
                    let mut line = format!("    {} {}", style("•").dim(), entry.name);
                    if let Some(restart) = entry.restart_service {
                        line.push_str(&format!(
                            " {}",
                            style(format!("(service: {:?})", restart)).dim()
                        ));
                    }
                    println!("{}", line);
                }
                println!();
            }

            if !plan.already_installed.is_empty() {
                println!(
                    "  {} ({}):",
                    style("Already installed").yellow(),
                    plan.already_installed.len()
                );
                for name in &plan.already_installed {
                    println!("    {} {}", style("✓").green(), name);
                }
                println!();
            }

            if !plan.unsupported.is_empty() {
                println!(
                    "  {} ({}):",
                    style("Unsupported (will skip)").red(),
                    plan.unsupported.len()
                );
                for entry in &plan.unsupported {
                    println!("    {} {}", style("✗").red(), entry);
                }
                println!();
            }

            if plan.is_empty() {
                println!("{} Nothing to install.", style("==>").cyan().bold());
                return Ok(());
            }

            if dry_run {
                println!(
                    "{} Dry run - no packages were installed.",
                    style("==>").cyan().bold()
                );
                println!(
                    "    Run {} to install {} packages.",
                    style(format!("zb import {}", brewfile.display())).cyan(),
                    plan.total_to_install()
                );
                return Ok(());
            }

            // Execute import
            let start = Instant::now();
            println!(
                "{} Installing {} packages from Brewfile...",
                style("==>").cyan().bold(),
                plan.total_to_install()
            );

            // Set up progress display
            let multi = MultiProgress::new();
            let bars: Arc<Mutex<HashMap<String, ProgressBar>>> =
                Arc::new(Mutex::new(HashMap::new()));

            let download_style = ProgressStyle::default_bar()
                .template(
                    "    {prefix:<16} {bar:25.cyan/dim} {bytes:>10}/{total_bytes:<10} {eta:>6}",
                )
                .unwrap()
                .progress_chars("━━╸");

            let spinner_style = ProgressStyle::default_spinner()
                .template("    {prefix:<16} {spinner:.cyan} {msg}")
                .unwrap()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏");

            let done_style = ProgressStyle::default_spinner()
                .template("    {prefix:<16} {msg}")
                .unwrap();

            let bars_clone = bars.clone();
            let multi_clone = multi.clone();
            let download_style_clone = download_style.clone();
            let spinner_style_clone = spinner_style.clone();
            let done_style_clone = done_style.clone();

            let progress_callback: Arc<ProgressCallback> = Arc::new(Box::new(move |event| {
                let mut bars = bars_clone.lock().unwrap();
                match event {
                    InstallProgress::DownloadStarted { name, total_bytes } => {
                        let pb = if let Some(total) = total_bytes {
                            let pb = multi_clone.add(ProgressBar::new(total));
                            pb.set_style(download_style_clone.clone());
                            pb
                        } else {
                            let pb = multi_clone.add(ProgressBar::new_spinner());
                            pb.set_style(spinner_style_clone.clone());
                            pb.set_message("downloading...");
                            pb.enable_steady_tick(std::time::Duration::from_millis(80));
                            pb
                        };
                        pb.set_prefix(name.clone());
                        bars.insert(name, pb);
                    }
                    InstallProgress::DownloadProgress {
                        name,
                        downloaded,
                        total_bytes,
                    } => {
                        if let Some(pb) = bars.get(&name)
                            && total_bytes.is_some()
                        {
                            pb.set_position(downloaded);
                        }
                    }
                    InstallProgress::DownloadCompleted { name, total_bytes } => {
                        if let Some(pb) = bars.get(&name) {
                            if total_bytes > 0 {
                                pb.set_position(total_bytes);
                            }
                            pb.set_style(spinner_style_clone.clone());
                            pb.set_message("unpacking...");
                            pb.enable_steady_tick(std::time::Duration::from_millis(80));
                        }
                    }
                    InstallProgress::UnpackStarted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_message("unpacking...");
                        }
                    }
                    InstallProgress::UnpackCompleted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_message("linking...");
                        }
                    }
                    InstallProgress::LinkStarted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_message("linking...");
                        }
                    }
                    InstallProgress::LinkCompleted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_style(done_style_clone.clone());
                            pb.set_message(format!("{} installed", style("✓").green()));
                            pb.finish();
                        }
                    }
                }
            }));

            let result = importer
                .execute_with_progress(plan, Some(progress_callback))
                .await
                .map_err(|e| zb_core::Error::StoreCorruption {
                    message: format!("Import failed: {}", e),
                })?;

            // Finish any remaining bars
            {
                let bars = bars.lock().unwrap();
                for (_, pb) in bars.iter() {
                    if !pb.is_finished() {
                        pb.finish();
                    }
                }
            }

            let elapsed = start.elapsed();
            println!();
            println!("{} Import complete!", style("==>").cyan().bold());
            println!(
                "    {} Installed: {} packages",
                style("✓").green(),
                style(result.installed.len()).green().bold()
            );

            if !result.services_enabled.is_empty() {
                println!(
                    "    {} Services enabled: {}",
                    style("✓").green(),
                    result.services_enabled.len()
                );
                for svc in &result.services_enabled {
                    println!("      {} {}", style("•").dim(), svc);
                }
            }

            if !result.failed.is_empty() {
                println!(
                    "    {} Failed: {} packages",
                    style("✗").red(),
                    result.failed.len()
                );
                for (name, err) in &result.failed {
                    println!("      {} {}: {}", style("•").dim(), name, style(err).dim());
                }
            }

            println!("    Time: {:.2}s", elapsed.as_secs_f64());
        }

        Commands::Export { output } => {
            println!(
                "{} Exporting packages to Brewfile...",
                style("==>").cyan().bold()
            );

            let exporter = Exporter::new(&installer);
            let brewfile_content =
                exporter
                    .to_string()
                    .map_err(|e| zb_core::Error::StoreCorruption {
                        message: format!("Failed to export Brewfile: {}", e),
                    })?;

            if let Some(output_path) = output {
                std::fs::write(&output_path, &brewfile_content).map_err(|e| {
                    zb_core::Error::StoreCorruption {
                        message: format!("Failed to write Brewfile: {}", e),
                    }
                })?;

                let brew_count = brewfile_content
                    .lines()
                    .filter(|l| l.starts_with("brew "))
                    .count();
                println!(
                    "{} Exported {} packages to {}",
                    style("==>").cyan().bold(),
                    style(brew_count).green().bold(),
                    output_path.display()
                );
            } else {
                // Write to stdout
                println!();
                print!("{}", brewfile_content);
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
