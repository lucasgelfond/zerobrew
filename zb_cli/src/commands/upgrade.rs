use console::style;
use zb_io::UpgradeResult;

pub async fn execute(
    installer: &mut zb_io::Installer,
    formulas: Vec<String>,
    dry_run: bool,
) -> Result<(), zb_core::Error> {
    // Determine which packages to upgrade
    let to_upgrade = if formulas.is_empty() {
        let (outdated, warnings) = installer.check_outdated().await?;
        for warning in &warnings {
            eprintln!("{} {}", style("Warning:").yellow().bold(), warning);
        }
        outdated
    } else {
        // Check specified formulas only
        let mut outdated = Vec::new();
        for name in &formulas {
            match installer.is_outdated(name).await {
                Ok(Some(pkg)) => outdated.push(pkg),
                Ok(None) => {
                    println!(
                        "{} {} is already up to date.",
                        style("==>").cyan().bold(),
                        name
                    );
                }
                Err(e) => {
                    eprintln!("{} {}: {}", style("✗").red().bold(), name, e);
                }
            }
        }
        outdated
    };

    if to_upgrade.is_empty() {
        println!("{} Nothing to upgrade.", style("==>").cyan().bold());
        return Ok(());
    }

    if dry_run {
        println!(
            "{} Would upgrade {} {}:",
            style("==>").cyan().bold(),
            to_upgrade.len(),
            if to_upgrade.len() == 1 {
                "package"
            } else {
                "packages"
            }
        );
        for pkg in &to_upgrade {
            println!(
                "  {} {} → {}",
                pkg.name, pkg.installed_version, pkg.current_version
            );
        }
        return Ok(());
    }

    let mut upgraded = 0;
    let mut errors = Vec::new();

    for pkg in &to_upgrade {
        println!(
            "{} Upgrading {} {} → {}...",
            style("==>").cyan().bold(),
            style(&pkg.name).bold(),
            pkg.installed_version,
            pkg.current_version,
        );

        match installer.upgrade_package(&pkg.name).await {
            Ok(UpgradeResult::Success) => {
                println!(
                    "  {} {} {} → {}",
                    style("✓").green().bold(),
                    style(&pkg.name).bold(),
                    pkg.installed_version,
                    pkg.current_version,
                );
                upgraded += 1;
            }
            Ok(UpgradeResult::FailedWithRollback { error }) => {
                eprintln!(
                    "  {} {}: {} (previous version restored)",
                    style("✗").red().bold(),
                    pkg.name,
                    error,
                );
                errors.push(pkg.name.clone());
            }
            Ok(UpgradeResult::FailedRollbackIncomplete { error }) => {
                eprintln!(
                    "  {} {}: {} (WARNING: rollback incomplete, run: zb uninstall {} && zb install {})",
                    style("✗").red().bold(),
                    pkg.name,
                    error,
                    pkg.name,
                    pkg.name,
                );
                errors.push(pkg.name.clone());
            }
            Err(e) => {
                eprintln!("  {} {}: {}", style("✗").red().bold(), pkg.name, e);
                errors.push(pkg.name.clone());
            }
        }
    }

    if upgraded > 0 {
        println!(
            "{} Upgraded {} {}.",
            style("==>").cyan().bold(),
            style(upgraded).green().bold(),
            if upgraded == 1 { "package" } else { "packages" }
        );
    }

    if !errors.is_empty() {
        eprintln!(
            "{} {} {} failed to upgrade: {}",
            style("==>").red().bold(),
            errors.len(),
            if errors.len() == 1 {
                "package"
            } else {
                "packages"
            },
            errors.join(", ")
        );
    }

    Ok(())
}
