use console::style;

pub fn execute(
    installer: &mut zb_io::Installer,
    formulas: Vec<String>,
    all: bool,
) -> Result<(), zb_core::Error> {
    let formulas = if all {
        let installed = installer.list_installed()?;
        if installed.is_empty() {
            println!("No formulas installed.");
            return Ok(());
        }
        installed.into_iter().map(|k| k.name).collect()
    } else {
        formulas
    };

    println!(
        "{} Uninstalling {}...",
        style("==>").cyan().bold(),
        style(formulas.join(", ")).bold()
    );

    let mut errors: Vec<(String, zb_core::Error)> = Vec::new();

    if formulas.len() > 1 {
        for name in &formulas {
            print!("    {} {}...", style("○").dim(), name);
            match installer.uninstall(name) {
                Ok(()) => println!(" {}", style("✓").green()),
                Err(e) => {
                    println!(" {}", style("✗").red());
                    errors.push((name.clone(), e));
                }
            }
        }
    } else if let Err(e) = installer.uninstall(&formulas[0]) {
        errors.push((formulas[0].clone(), e));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        for (name, err) in &errors {
            eprintln!(
                "{} Failed to uninstall {}: {}",
                style("Error:").red().bold(),
                style(name).bold(),
                err
            );
        }
        // Return just the first error up. TODO: don't return errors from this fn?
        Err(errors.remove(0).1)
    }
}
