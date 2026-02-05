use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use zb_io::{InstallProgress, ProgressCallback};

use crate::utils::{normalize_formula_name, suggest_homebrew};

pub async fn execute(
    installer: &mut zb_io::Installer,
    formulas: Vec<String>,
    no_link: bool,
) -> Result<(), zb_core::Error> {
    let start = Instant::now();
    println!(
        "{} Installing {}...",
        style("==>").cyan().bold(),
        style(formulas.join(", ")).bold()
    );

    let mut normalized_names = Vec::new();
    for formula in &formulas {
        match normalize_formula_name(formula) {
            Ok(name) => normalized_names.push(name),
            Err(e) => {
                suggest_homebrew(formula, &e);
                return Err(e);
            }
        }
    }

    let plan = match installer.plan(&normalized_names).await {
        Ok(p) => p,
        Err(e) => {
            for formula in &formulas {
                suggest_homebrew(formula, &e);
            }
            return Err(e);
        }
    };

    println!(
        "{} Resolving dependencies ({} packages)...",
        style("==>").cyan().bold(),
        plan.formulas.len()
    );
    for f in &plan.formulas {
        println!(
            "    {} {}",
            style(&f.name).green(),
            style(&f.versions.stable).dim()
        );
    }

    let multi = MultiProgress::new();
    let bars: Arc<Mutex<HashMap<String, ProgressBar>>> = Arc::new(Mutex::new(HashMap::new()));

    let download_style = ProgressStyle::default_bar()
        .template("    {prefix:<16} {bar:25.cyan/dim} {bytes:>10}/{total_bytes:<10} {eta:>6}")
        .unwrap()
        .progress_chars("━━╸");

    let spinner_style = ProgressStyle::default_spinner()
        .template("    {prefix:<16} {spinner:.cyan} {msg}")
        .unwrap()
        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏");

    let done_style = ProgressStyle::default_spinner()
        .template("    {prefix:<16} {msg}")
        .unwrap();

    println!(
        "{} Downloading and installing...",
        style("==>").cyan().bold()
    );

    let bars_clone = bars.clone();
    let multi_clone = multi.clone();
    let download_style_clone = download_style.clone();
    let spinner_style_clone = spinner_style.clone();
    let done_style_clone = done_style.clone();

    let progress_callback: Arc<ProgressCallback> = Arc::new(Box::new(move |event| {
        let mut bars = bars_clone.lock().unwrap();
        match event {
            InstallProgress::DownloadStarted { name, total_bytes } => {
                let pb = if let Some(total) = total_bytes {
                    let pb = multi_clone.add(ProgressBar::new(total));
                    pb.set_style(download_style_clone.clone());
                    pb
                } else {
                    let pb = multi_clone.add(ProgressBar::new_spinner());
                    pb.set_style(spinner_style_clone.clone());
                    pb.set_message("downloading...");
                    pb.enable_steady_tick(std::time::Duration::from_millis(80));
                    pb
                };
                pb.set_prefix(name.clone());
                bars.insert(name, pb);
            }
            InstallProgress::DownloadProgress {
                name,
                downloaded,
                total_bytes,
            } => {
                if let Some(pb) = bars.get(&name)
                    && total_bytes.is_some()
                {
                    pb.set_position(downloaded);
                }
            }
            InstallProgress::DownloadCompleted { name, total_bytes } => {
                if let Some(pb) = bars.get(&name) {
                    if total_bytes > 0 {
                        pb.set_position(total_bytes);
                    }
                    pb.set_style(spinner_style_clone.clone());
                    pb.set_message("unpacking...");
                    pb.enable_steady_tick(std::time::Duration::from_millis(80));
                }
            }
            InstallProgress::UnpackStarted { name } => {
                if let Some(pb) = bars.get(&name) {
                    pb.set_message("unpacking...");
                }
            }
            InstallProgress::UnpackCompleted { name } => {
                if let Some(pb) = bars.get(&name) {
                    pb.set_message("unpacked");
                }
            }
            InstallProgress::LinkStarted { name } => {
                if let Some(pb) = bars.get(&name) {
                    pb.set_message("linking...");
                }
            }
            InstallProgress::LinkCompleted { name } => {
                if let Some(pb) = bars.get(&name) {
                    pb.set_message("linked");
                }
            }
            InstallProgress::InstallCompleted { name } => {
                if let Some(pb) = bars.get(&name) {
                    pb.set_style(done_style_clone.clone());
                    pb.set_message(format!("{} installed", style("✓").green()));
                    pb.finish();
                }
            }
        }
    }));

    let result_val = installer
        .execute_with_progress(plan, !no_link, Some(progress_callback))
        .await;

    {
        let bars = bars.lock().unwrap();
        for (_, pb) in bars.iter() {
            if !pb.is_finished() {
                pb.finish();
            }
        }
    }

    let result = match result_val {
        Ok(r) => r,
        Err(e) => {
            for formula in &formulas {
                suggest_homebrew(formula, &e);
            }
            return Err(e);
        }
    };

    let elapsed = start.elapsed();
    println!();
    println!(
        "{} Installed {} packages in {:.2}s",
        style("==>").cyan().bold(),
        style(result.installed).green().bold(),
        elapsed.as_secs_f64()
    );

    Ok(())
}
