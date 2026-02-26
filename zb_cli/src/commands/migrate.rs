use crate::ui::{PromptDefault, Ui};
use console::style;
use std::process::Command;

pub async fn execute(
    installer: &mut zb_io::Installer,
    yes: bool,
    force: bool,
) -> Result<(), zb_core::Error> {
    let mut ui = Ui::new();

    ui.heading("Fetching installed Homebrew packages...")
        .map_err(ui_error)?;

    let packages = match zb_io::get_homebrew_packages() {
        Ok(pkgs) => pkgs,
        Err(e) => {
            return Err(zb_core::Error::StoreCorruption {
                message: format!("Failed to get Homebrew packages: {}", e),
            });
        }
    };

    if packages.formulas.is_empty()
        && packages.non_core_formulas.is_empty()
        && packages.casks.is_empty()
    {
        ui.info("No Homebrew packages installed.")
            .map_err(ui_error)?;
        return Ok(());
    }

    ui.info(format!(
        "{} core formulas, {} non-core formulas, {} casks found",
        style(packages.formulas.len()).green(),
        style(packages.non_core_formulas.len()).yellow(),
        style(packages.casks.len()).green()
    ))
    .map_err(ui_error)?;
    ui.blank_line().map_err(ui_error)?;

    if !packages.non_core_formulas.is_empty() {
        ui.note("Formulas from non-core taps cannot be migrated to zerobrew:")
            .map_err(ui_error)?;
        for pkg in &packages.non_core_formulas {
            ui.bullet(format!("{} ({})", pkg.name, pkg.tap))
                .map_err(ui_error)?;
        }
        ui.blank_line().map_err(ui_error)?;
    }

    if !packages.casks.is_empty() {
        ui.note("Casks cannot be migrated to zerobrew (only CLI formulas are supported):")
            .map_err(ui_error)?;
        for cask in &packages.casks {
            ui.bullet(&cask.name).map_err(ui_error)?;
        }
        ui.blank_line().map_err(ui_error)?;
    }

    if packages.formulas.is_empty() {
        ui.info("No core formulas to migrate.").map_err(ui_error)?;
        return Ok(());
    }

    ui.info(format!(
        "The following {} formulas will be migrated:",
        packages.formulas.len()
    ))
    .map_err(ui_error)?;
    for pkg in &packages.formulas {
        ui.bullet(&pkg.name).map_err(ui_error)?;
    }
    ui.blank_line().map_err(ui_error)?;

    if !yes
        && !ui
            .prompt_yes_no("Continue with migration? [y/N]", PromptDefault::No)
            .map_err(ui_error)?
    {
        ui.info("Aborted.").map_err(ui_error)?;
        return Ok(());
    }

    ui.blank_line().map_err(ui_error)?;
    ui.heading(format!(
        "Migrating {} formulas to zerobrew...",
        style(packages.formulas.len()).green().bold()
    ))
    .map_err(ui_error)?;

    let mut success_count = 0;
    let mut failed: Vec<String> = Vec::new();

    for pkg in &packages.formulas {
        ui.step_start(&pkg.name).map_err(ui_error)?;

        match installer.plan(std::slice::from_ref(&pkg.name)).await {
            Ok(plan) => match installer.execute(plan, true).await {
                Ok(_) => {
                    ui.step_ok().map_err(ui_error)?;
                    success_count += 1;
                }
                Err(e) => {
                    ui.step_fail().map_err(ui_error)?;
                    ui.error(format!("Failed to install: {}", e))
                        .map_err(ui_error)?;
                    failed.push(pkg.name.clone());
                }
            },
            Err(e) => {
                ui.step_fail().map_err(ui_error)?;
                ui.error(format!("Failed to plan: {}", e))
                    .map_err(ui_error)?;
                failed.push(pkg.name.clone());
            }
        }
    }

    ui.blank_line().map_err(ui_error)?;
    ui.heading(format!(
        "Migrated {} of {} formulas to zerobrew",
        style(success_count).green().bold(),
        packages.formulas.len()
    ))
    .map_err(ui_error)?;

    if !failed.is_empty() {
        ui.note(format!("Failed to migrate {} formula(s):", failed.len()))
            .map_err(ui_error)?;
        for name in &failed {
            ui.bullet(name).map_err(ui_error)?;
        }
        ui.blank_line().map_err(ui_error)?;
    }

    if success_count == 0 {
        ui.info("No formulas were successfully migrated. Skipping uninstall from Homebrew.")
            .map_err(ui_error)?;
        return Ok(());
    }

    ui.blank_line().map_err(ui_error)?;
    if !yes
        && !ui
            .prompt_yes_no(
                &format!(
                    "Uninstall {} formula(s) from Homebrew? [y/N]",
                    style(success_count).green()
                ),
                PromptDefault::No,
            )
            .map_err(ui_error)?
    {
        ui.info("Skipped uninstall from Homebrew.")
            .map_err(ui_error)?;
        return Ok(());
    }

    ui.blank_line().map_err(ui_error)?;
    ui.heading("Uninstalling from Homebrew...")
        .map_err(ui_error)?;

    let mut uninstalled = 0;
    let mut uninstall_failed: Vec<String> = Vec::new();

    for pkg in &packages.formulas {
        if failed.contains(&pkg.name) {
            continue;
        }

        ui.step_start(&pkg.name).map_err(ui_error)?;

        let mut args = vec!["uninstall"];
        if force {
            args.push("--force");
        }
        args.push(&pkg.name);

        let status = Command::new("brew")
            .args(&args)
            .status()
            .map_err(|e| format!("Failed to run brew uninstall: {}", e));

        match status {
            Ok(s) if s.success() => {
                ui.step_ok().map_err(ui_error)?;
                uninstalled += 1;
            }
            Ok(_) => {
                ui.step_fail().map_err(ui_error)?;
                uninstall_failed.push(pkg.name.clone());
            }
            Err(e) => {
                ui.step_fail().map_err(ui_error)?;
                ui.error(e).map_err(ui_error)?;
                uninstall_failed.push(pkg.name.clone());
            }
        }
    }

    ui.blank_line().map_err(ui_error)?;
    ui.heading(format!(
        "Uninstalled {} of {} formula(s) from Homebrew",
        style(uninstalled).green().bold(),
        success_count
    ))
    .map_err(ui_error)?;

    if !uninstall_failed.is_empty() {
        ui.note(format!(
            "Failed to uninstall {} formula(s) from Homebrew:",
            uninstall_failed.len()
        ))
        .map_err(ui_error)?;
        for name in &uninstall_failed {
            ui.bullet(name).map_err(ui_error)?;
        }
        ui.info("You may need to uninstall these manually with:")
            .map_err(ui_error)?;
        ui.info("    brew uninstall --force <formula>")
            .map_err(ui_error)?;
    }

    Ok(())
}

fn ui_error(err: std::io::Error) -> zb_core::Error {
    zb_core::Error::StoreCorruption {
        message: format!("failed to write CLI output: {err}"),
    }
}
