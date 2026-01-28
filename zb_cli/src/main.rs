use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::generate;
use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use zb_io::install::create_installer;
use zb_io::{InstallProgress, ProgressCallback};

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

    /// Migrate all installed Homebrew packages to zerobrew
    Migrate {
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
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

    /// Generate shell completion scripts
    Completion {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::shells::Shell,
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
        let zdotdir = std::env::var("ZDOTDIR").unwrap_or_else(|_| home.clone());
        let zshenv = format!("{}/.zshenv", zdotdir);

        // Prefer .zshenv (sourced for all shells), fall back to .zshrc
        if std::path::Path::new(&zshenv).exists() {
            zshenv
        } else {
            format!("{}/.zshrc", zdotdir)
        }
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

        let write_result = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config_file)
            .and_then(|mut f| f.write_all(addition.as_bytes()));

        if let Err(e) = write_result {
            println!(
                "{} Could not write to {} due to error: {}",
                style("Warning:").yellow().bold(),
                config_file,
                e
            );
            println!(
                "{} Please add the following line to {}:",
                style("Info:").cyan().bold(),
                config_file
            );
            println!("{}", addition);
        } else {
            println!(
                "    {} Added {} to PATH in {}",
                style("✓").green(),
                bin_path.display(),
                config_file
            );
        }
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

fn normalize_formula_name(name: &str) -> Result<String, zb_core::Error> {
    let trimmed = name.trim();
    if let Some((tap, formula)) = trimmed.rsplit_once('/') {
        if tap == "homebrew/core" {
            if formula.is_empty() {
                return Err(zb_core::Error::MissingFormula {
                    name: trimmed.to_string(),
                });
            }
            return Ok(formula.to_string());
        }
        return Err(zb_core::Error::UnsupportedTap {
            name: trimmed.to_string(),
        });
    }

    Ok(trimmed.to_string())
}

/// Represents a Homebrew package that can be migrated
struct HomebrewPackage {
    name: String,
    #[allow(dead_code)]
    tap: String,
    is_cask: bool,
}

fn get_homebrew_packages() -> Result<Vec<HomebrewPackage>, String> {
    // Get installed formulas
    let formulas_output = Command::new("brew")
        .args(["info", "--json=v1", "--installed"])
        .output()
        .map_err(|e| format!("Failed to run 'brew info': {}", e))?;

    if !formulas_output.status.success() {
        return Err(format!(
            "brew info failed: {}",
            String::from_utf8_lossy(&formulas_output.stderr)
        ));
    }

    let formulas_json: serde_json::Value = serde_json::from_slice(&formulas_output.stdout)
        .map_err(|e| format!("Failed to parse brew info JSON: {}", e))?;

    let mut packages = Vec::new();

    if let Some(formulas) = formulas_json.as_array() {
        for formula in formulas {
            if let Some(name) = formula.get("name").and_then(|n| n.as_str()) {
                let tap = formula
                    .get("tap")
                    .and_then(|t| t.as_str())
                    .unwrap_or("homebrew/core")
                    .to_string();
                packages.push(HomebrewPackage {
                    name: name.to_string(),
                    tap,
                    is_cask: false,
                });
            }
        }
    }

    // Get installed casks
    let casks_output = Command::new("brew")
        .args(["list", "--cask", "--json=v1"])
        .output()
        .map_err(|e| format!("Failed to run 'brew list --cask': {}", e))?;

    if casks_output.status.success()
        && let Ok(casks_json) = serde_json::from_slice::<serde_json::Value>(&casks_output.stdout)
        && let Some(casks) = casks_json.as_array()
    {
        for cask in casks {
            if let Some(token) = cask.get("token").and_then(|t| t.as_str()) {
                packages.push(HomebrewPackage {
                    name: token.to_string(),
                    tap: "homebrew/cask".to_string(),
                    is_cask: true,
                });
            }
        }
    }

    Ok(packages)
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

async fn run(cli: Cli) -> Result<(), zb_core::Error> {
    // Handle completion first - it doesn't need the installer
    if let Commands::Completion { shell } = cli.command {
        let mut cmd = Cli::command();
        generate(shell, &mut cmd, "zb", &mut io::stdout());
        return Ok(());
    }

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
        Commands::Init => unreachable!(),              // Handled above
        Commands::Completion { .. } => unreachable!(), // Handled above
        Commands::Install { formula, no_link } => {
            let start = Instant::now();
            println!(
                "{} Installing {}...",
                style("==>").cyan().bold(),
                style(&formula).bold()
            );

            let normalized = match normalize_formula_name(&formula) {
                Ok(name) => name,
                Err(e) => {
                    suggest_homebrew(&formula, &e);
                    return Err(e);
                }
            };

            let plan = match installer.plan(&normalized).await {
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
                            pb.set_message("unpacked");
                        }
                    }
                    InstallProgress::LinkStarted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_message("linking...");
                        }
                    }
                    InstallProgress::LinkCompleted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_message("linked");
                        }
                    }
                    InstallProgress::InstallCompleted { name } => {
                        if let Some(pb) = bars.get(&name) {
                            pb.set_style(done_style_clone.clone());
                            pb.set_message(format!("{} installed", style("✓").green()));
                            pb.finish();
                        }
                    }
                }
            }));

            let result_val = installer
                .execute_with_progress(plan, !no_link, Some(progress_callback))
                .await;

            // Cleanup progress bars BEFORE handling errors to avoid visual artifacts
            {
                let bars = bars.lock().unwrap();
                for (_, pb) in bars.iter() {
                    if !pb.is_finished() {
                        pb.finish();
                    }
                }
            }

            // Now handle the result
            let result = match result_val {
                Ok(r) => r,
                Err(e) => {
                    suggest_homebrew(&formula, &e);
                    return Err(e);
                }
            };

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

        Commands::Migrate { yes } => {
            println!(
                "{} Fetching installed Homebrew packages...",
                style("==>").cyan().bold()
            );

            let packages = match get_homebrew_packages() {
                Ok(pkgs) => pkgs,
                Err(e) => {
                    return Err(zb_core::Error::StoreCorruption {
                        message: format!("Failed to get Homebrew packages: {}", e),
                    });
                }
            };

            if packages.is_empty() {
                println!("No Homebrew packages installed.");
                return Ok(());
            }

            let formulas: Vec<_> = packages.iter().filter(|p| !p.is_cask).collect();
            let casks: Vec<_> = packages.iter().filter(|p| p.is_cask).collect();

            println!(
                "    {} formulas, {} casks found",
                style(formulas.len()).green(),
                style(casks.len()).green()
            );
            println!();

            // Show casks that can't be migrated
            if !casks.is_empty() {
                println!(
                    "{} Casks cannot be migrated to zerobrew (only CLI formulas are supported):",
                    style("Note:").yellow().bold()
                );
                for cask in &casks {
                    println!("    • {}", cask.name);
                }
                println!();
            }

            if formulas.is_empty() {
                println!("No formulas to migrate.");
                return Ok(());
            }

            println!(
                "The following {} formulas will be migrated:",
                formulas.len()
            );
            for pkg in &formulas {
                println!("    • {}", pkg.name);
            }
            println!();

            if !yes {
                print!("Continue with migration? [y/N] ");
                io::stdout().flush().unwrap();

                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            println!();
            println!(
                "{} Migrating {} formulas to zerobrew...",
                style("==>").cyan().bold(),
                style(formulas.len()).green().bold()
            );

            let mut success_count = 0;
            let mut failed: Vec<String> = Vec::new();

            for pkg in &formulas {
                print!("    {} {}...", style("○").dim(), pkg.name);

                match installer.plan(&pkg.name).await {
                    Ok(plan) => {
                        // Execute the plan without progress bars for batch migration
                        match installer.execute(plan, true).await {
                            Ok(_) => {
                                println!(" {}", style("✓").green());
                                success_count += 1;
                            }
                            Err(e) => {
                                println!(" {}", style("✗").red());
                                eprintln!(
                                    "      {} Failed to install: {}",
                                    style("error:").red().bold(),
                                    e
                                );
                                failed.push(pkg.name.clone());
                            }
                        }
                    }
                    Err(e) => {
                        println!(" {}", style("✗").red());
                        eprintln!(
                            "      {} Failed to plan: {}",
                            style("error:").red().bold(),
                            e
                        );
                        failed.push(pkg.name.clone());
                    }
                }
            }

            println!();
            println!(
                "{} Migrated {} of {} formulas to zerobrew",
                style("==>").cyan().bold(),
                style(success_count).green().bold(),
                formulas.len()
            );

            if !failed.is_empty() {
                println!(
                    "{} Failed to migrate {} formula(s):",
                    style("Warning:").yellow().bold(),
                    failed.len()
                );
                for name in &failed {
                    println!("    • {}", name);
                }
                println!();
            }

            if success_count == 0 {
                println!(
                    "No formulas were successfully migrated. Skipping uninstall from Homebrew."
                );
                return Ok(());
            }

            // Ask for confirmation to uninstall from Homebrew
            println!();
            if !yes {
                print!(
                    "Uninstall {} formula(s) from Homebrew? [y/N] ",
                    style(success_count).green()
                );
                io::stdout().flush().unwrap();

                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Skipped uninstall from Homebrew.");
                    return Ok(());
                }
            }

            println!();
            println!(
                "{} Uninstalling from Homebrew...",
                style("==>").cyan().bold()
            );

            let mut uninstalled = 0;
            let mut uninstall_failed: Vec<String> = Vec::new();

            for pkg in &formulas {
                // Skip if it failed to install in zerobrew
                if failed.contains(&pkg.name) {
                    continue;
                }

                print!("    {} {}...", style("○").dim(), pkg.name);

                let status = Command::new("brew")
                    .args(["uninstall", "--force", &pkg.name])
                    .status()
                    .map_err(|e| format!("Failed to run brew uninstall: {}", e));

                match status {
                    Ok(s) if s.success() => {
                        println!(" {}", style("✓").green());
                        uninstalled += 1;
                    }
                    Ok(_) => {
                        println!(" {}", style("✗").red());
                        uninstall_failed.push(pkg.name.clone());
                    }
                    Err(e) => {
                        println!(" {}", style("✗").red());
                        eprintln!("      {}: {}", style("error:").red().bold(), e);
                        uninstall_failed.push(pkg.name.clone());
                    }
                }
            }

            println!();
            println!(
                "{} Uninstalled {} of {} formula(s) from Homebrew",
                style("==>").cyan().bold(),
                style(uninstalled).green().bold(),
                success_count
            );

            if !uninstall_failed.is_empty() {
                println!(
                    "{} Failed to uninstall {} formula(s) from Homebrew:",
                    style("Warning:").yellow().bold(),
                    uninstall_failed.len()
                );
                for name in &uninstall_failed {
                    println!("    • {}", name);
                }
                println!("You may need to uninstall these manually with:");
                println!("    brew uninstall --force <formula>");
            }
        }

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
    }

    Ok(())
}

fn chrono_lite_format(timestamp: i64) -> String {
    // Simple timestamp formatting without pulling in chrono
    use std::time::{Duration, UNIX_EPOCH};

    let dt = UNIX_EPOCH + Duration::from_secs(timestamp as u64);
    format!("{:?}", dt)
}
