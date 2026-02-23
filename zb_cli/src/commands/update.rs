use console::style;

pub fn execute(installer: &mut zb_io::Installer) -> Result<(), zb_core::Error> {
    let removed = installer.clear_api_cache()?;
    if removed == 0 {
        println!("{} No cached entries to clear.", style("==>").cyan().bold());
    } else {
        println!(
            "{} Cleared {} cached formula {}.",
            style("==>").cyan().bold(),
            style(removed).green().bold(),
            if removed == 1 { "entry" } else { "entries" }
        );
    }
    Ok(())
}
