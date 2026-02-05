use console::style;

pub fn execute(installer: &mut zb_io::Installer) -> Result<(), zb_core::Error> {
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

    Ok(())
}
