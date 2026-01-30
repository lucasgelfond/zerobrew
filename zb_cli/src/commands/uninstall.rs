use console::style;

pub fn execute(
    installer: &mut zb_io::install::Installer,
    formula: Option<String>,
) -> Result<(), zb_core::Error> {
    match formula {
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
    }
    Ok(())
}
