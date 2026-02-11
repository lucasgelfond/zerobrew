use console::style;

pub fn execute(installer: &mut zb_io::Installer) -> Result<(), zb_core::Error> {
    println!(
        "{} Scanning cellar for unregistered kegs...",
        style("==>").cyan().bold()
    );

    let repaired = installer.repair()?;

    if repaired.is_empty() {
        println!("All kegs are registered in the database. Nothing to repair.");
    } else {
        println!(
            "{} Registered {} keg(s) in the database:",
            style("==>").cyan().bold(),
            style(repaired.len()).green().bold()
        );
        for (name, version) in &repaired {
            println!(
                "    {} {} {}",
                style("✓").green(),
                name,
                style(version).dim()
            );
        }
    }

    Ok(())
}
