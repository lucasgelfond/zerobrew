use console::style;
use std::io::{self, Write};
use std::process::Command;

pub async fn execute(
    installer: &mut zb_io::Installer,
    yes: bool,
    force: bool,
) -> Result<(), zb_core::Error> {
    println!(
        "{} Fetching installed Homebrew packages...",
        style("==>").cyan().bold()
    );

    let packages = match zb_io::get_homebrew_packages() {
        Ok(pkgs) => pkgs,
        Err(e) => {
            return Err(zb_core::Error::StoreCorruption {
                message: format!("Failed to get Homebrew packages: {}", e),
            });
        }
    };

    if packages.formulas.is_empty()
        && packages.non_core_formulas.is_empty()
        && packages.casks.is_empty()
    {
        println!("No Homebrew packages installed.");
        return Ok(());
    }

    println!(
        "    {} core formulas, {} non-core formulas, {} casks found",
        style(packages.formulas.len()).green(),
        style(packages.non_core_formulas.len()).yellow(),
        style(packages.casks.len()).green()
    );
    println!();

    if !packages.non_core_formulas.is_empty() {
        println!(
            "{} Formulas from non-core taps cannot be migrated to zerobrew:",
            style("Note:").yellow().bold()
        );
        for pkg in &packages.non_core_formulas {
            println!("    • {} ({})", pkg.name, pkg.tap);
        }
        println!();
    }

    if !packages.casks.is_empty() {
        println!(
            "{} Casks cannot be migrated to zerobrew (only CLI formulas are supported):",
            style("Note:").yellow().bold()
        );
        for cask in &packages.casks {
            println!("    • {}", cask.name);
        }
        println!();
    }

    if packages.formulas.is_empty() {
        println!("No core formulas to migrate.");
        return Ok(());
    }

    println!(
        "The following {} formulas will be migrated:",
        packages.formulas.len()
    );
    for pkg in &packages.formulas {
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
        style(packages.formulas.len()).green().bold()
    );

    let mut success_count = 0;
    let mut failed: Vec<String> = Vec::new();

    for pkg in &packages.formulas {
        print!("    {} {}...", style("○").dim(), pkg.name);

        match installer.plan(std::slice::from_ref(&pkg.name)).await {
            Ok(plan) => match installer.execute(plan, true).await {
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
            },
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
        packages.formulas.len()
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
        println!("No formulas were successfully migrated. Skipping uninstall from Homebrew.");
        return Ok(());
    }

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

    for pkg in &packages.formulas {
        if failed.contains(&pkg.name) {
            continue;
        }

        print!("    {} {}...", style("○").dim(), pkg.name);

        let mut args = vec!["uninstall"];
        if force {
            args.push("--force");
        }
        args.push(&pkg.name);

        let status = Command::new("brew")
            .args(&args)
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

    Ok(())
}
