use console::style;

pub async fn execute(
    installer: &mut zb_io::Installer,
    quiet: bool,
    verbose: bool,
    json: bool,
) -> Result<(), zb_core::Error> {
    let (outdated, warnings) = installer.check_outdated().await?;

    // Warnings always go to stderr (never pollute stdout, especially in --json mode)
    for warning in &warnings {
        eprintln!("{} {}", style("Warning:").yellow().bold(), warning);
    }

    if json {
        let json_output: Vec<serde_json::Value> = outdated
            .iter()
            .map(|pkg| {
                serde_json::json!({
                    "name": pkg.name,
                    "installed_versions": [pkg.installed_version],
                    "current_version": pkg.current_version,
                    "pinned": false,
                    "pinned_version": null,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_output).unwrap());
        return Ok(());
    }

    if outdated.is_empty() {
        if !quiet {
            println!(
                "{} All packages are up to date.",
                style("==>").cyan().bold()
            );
        }
        return Ok(());
    }

    for pkg in &outdated {
        if quiet {
            println!("{}", pkg.name);
        } else if verbose {
            println!(
                "{} {} {} {}",
                pkg.name,
                style(&pkg.installed_version).red(),
                style("→").dim(),
                style(&pkg.current_version).green(),
            );
        } else {
            println!(
                "{} ({}) < {}",
                pkg.name, pkg.installed_version, pkg.current_version
            );
        }
    }

    Ok(())
}
