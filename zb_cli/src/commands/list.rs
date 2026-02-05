use console::style;

pub fn execute(installer: &mut zb_io::Installer) -> Result<(), zb_core::Error> {
    let installed = installer.list_installed()?;

    if installed.is_empty() {
        println!("No formulas installed.");
    } else {
        for keg in installed {
            println!("{} {}", style(&keg.name).bold(), style(&keg.version).dim());
        }
    }

    Ok(())
}
