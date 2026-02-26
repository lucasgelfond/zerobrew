use crate::ui::{PromptDefault, Ui};
use std::path::Path;
use std::process::Command;

use crate::init::{InitError, run_init};

pub fn execute(root: &Path, prefix: &Path, yes: bool) -> Result<(), zb_core::Error> {
    let mut ui = Ui::new();

    if !root.exists() && !prefix.exists() {
        ui.info("Nothing to reset - directories do not exist.")
            .map_err(ui_error)?;
        return Ok(());
    }

    if !yes {
        ui.note("This will delete all zerobrew data at:")
            .map_err(ui_error)?;
        ui.bullet(root.display()).map_err(ui_error)?;
        ui.bullet(prefix.display()).map_err(ui_error)?;

        if !ui
            .prompt_yes_no("Continue? [y/N]", PromptDefault::No)
            .map_err(ui_error)?
        {
            ui.info("Aborted.").map_err(ui_error)?;
            return Ok(());
        }
    }

    for dir in [root, prefix] {
        if !dir.exists() {
            continue;
        }

        ui.heading(format!("Clearing {}...", dir.display()))
            .map_err(ui_error)?;

        // Instead of removing the directory entirely (which would require sudo to recreate),
        // just remove its contents. This avoids needing sudo when run_init recreates subdirs.
        let mut failed = false;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let result = if path.is_dir() {
                    std::fs::remove_dir_all(&path)
                } else {
                    std::fs::remove_file(&path)
                };
                if result.is_err() {
                    failed = true;
                    break;
                }
            }
        } else {
            failed = true;
        }

        // Only fall back to sudo if we couldn't clear contents AND stdout is a terminal
        if failed {
            if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                let _ = ui.error(format!(
                    "Failed to clear {} (permission denied, non-interactive mode)",
                    dir.display()
                ));
                std::process::exit(1);
            }

            // Interactive mode: fall back to sudo for the entire directory
            let status = Command::new("sudo")
                .args(["rm", "-rf", &dir.to_string_lossy()])
                .status();

            if status.is_err() || !status.unwrap().success() {
                let _ = ui.error(format!("Failed to remove {}", dir.display()));
                std::process::exit(1);
            }
        }
    }

    // Pass false for no_modify_shell since this is a re-initialization
    run_init(root, prefix, false).map_err(|e| match e {
        InitError::Message(msg) => zb_core::Error::StoreCorruption { message: msg },
    })?;

    ui.heading("Reset complete. Ready for cold install.")
        .map_err(ui_error)?;

    Ok(())
}

fn ui_error(err: std::io::Error) -> zb_core::Error {
    zb_core::Error::StoreCorruption {
        message: format!("failed to write CLI output: {err}"),
    }
}
