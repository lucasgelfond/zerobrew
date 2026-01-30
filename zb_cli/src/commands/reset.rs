use console::style;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

use crate::init::{InitError, run_init};

pub fn execute(root: &Path, prefix: &Path, yes: bool) -> Result<(), zb_core::Error> {
    if !root.exists() && !prefix.exists() {
        println!("Nothing to reset - directories do not exist.");
        return Ok(());
    }

    if !yes {
        println!(
            "{} This will delete all zerobrew data at:",
            style("Warning:").yellow().bold()
        );
        println!("      • {}", root.display());
        println!("      • {}", prefix.display());
        print!("Continue? [y/N] ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    for dir in [root, prefix] {
        if !dir.exists() {
            continue;
        }

        println!(
            "{} Removing {}...",
            style("==>").cyan().bold(),
            dir.display()
        );

        if std::fs::remove_dir_all(dir).is_err() {
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

    run_init(root, prefix).map_err(|e| match e {
        InitError::Message(msg) => zb_core::Error::StoreCorruption { message: msg },
    })?;

    println!(
        "{} Reset complete. Ready for cold install.",
        style("==>").cyan().bold()
    );

    Ok(())
}
