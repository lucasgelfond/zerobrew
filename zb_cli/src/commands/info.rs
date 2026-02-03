use chrono::{DateTime, Local};
use console::style;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use zb_io::DependencyKind;

pub fn execute(
    installer: &mut zb_io::install::Installer,
    formula: Option<String>,
) -> Result<(), zb_core::Error> {
    match formula {
        None => print_summary(installer),
        Some(name) => print_keg_info(installer, name),
    }
}

fn print_summary(installer: &mut zb_io::install::Installer) -> Result<(), zb_core::Error> {
    let installed = installer.list_installed()?;
    if installed.is_empty() {
        println!("No formulas installed.");
        return Ok(());
    }

    let mut total_files = 0u64;
    let mut total_bytes = 0u64;

    for keg in &installed {
        let keg_path = installer.keg_path(&keg.name, &keg.version);
        let (files, bytes) = collect_stats(&keg_path)?;
        total_files += files;
        total_bytes += bytes;
    }

    println!(
        "{} kegs, {} files, {}",
        format_count(installed.len() as u64),
        format_count(total_files),
        format_size(total_bytes)
    );

    Ok(())
}

fn print_keg_info(
    installer: &mut zb_io::install::Installer,
    formula: String,
) -> Result<(), zb_core::Error> {
    let Some(keg) = installer.get_installed(&formula) else {
        println!("Formula '{}' is not installed.", formula);
        return Ok(());
    };

    print_field("Name:", style(&keg.name).bold());
    print_field("Version:", &keg.version);
    print_field("Store key:", &keg.store_key[..12]);
    print_field("Installed:", format_timestamp(keg.installed_at));

    let keg_path = installer.keg_path(&keg.name, &keg.version);
    let (files, bytes) = collect_stats(&keg_path)?;
    println!(
        "\n{} ({} files, {})",
        keg_path.display(),
        format_count(files),
        format_size(bytes)
    );

    let installed = installer.list_installed()?;
    let installed_set: BTreeSet<String> = installed.into_iter().map(|k| k.name).collect();

    let dependency_rows = installer.list_dependencies_for(&keg.name)?;
    let mut build_deps: BTreeSet<String> = BTreeSet::new();
    let mut required_deps: BTreeSet<String> = BTreeSet::new();

    for row in dependency_rows {
        match row.kind {
            DependencyKind::Build => {
                build_deps.insert(row.dependency);
            }
            DependencyKind::Required => {
                required_deps.insert(row.dependency);
            }
        }
    }

    println!("{} Dependencies", style("==>").cyan().bold());
    if build_deps.is_empty() && required_deps.is_empty() {
        println!("(no dependency metadata; reinstall to populate)");
    } else {
        println!("Build: {}", format_dep_line(&build_deps, &installed_set));
        println!(
            "Required: {}",
            format_dep_line(&required_deps, &installed_set)
        );
    }

    Ok(())
}

fn print_field(label: &str, value: impl std::fmt::Display) {
    println!("{:<10}  {}", style(label).dim(), value);
}

fn format_timestamp(timestamp: i64) -> String {
    match DateTime::from_timestamp(timestamp, 0) {
        Some(dt) => {
            let local_dt = dt.with_timezone(&Local);
            let now = Local::now();
            let duration = now.signed_duration_since(local_dt);

            if duration.num_days() > 0 {
                format!(
                    "{} ({} days ago)",
                    local_dt.format("%Y-%m-%d"),
                    duration.num_days()
                )
            } else if duration.num_hours() > 0 {
                format!(
                    "{} ({} hours ago)",
                    local_dt.format("%Y-%m-%d %H:%M"),
                    duration.num_hours()
                )
            } else {
                format!(
                    "{} ({} minutes ago)",
                    local_dt.format("%H:%M"),
                    duration.num_minutes()
                )
            }
        }
        None => "invalid timestamp".to_string(),
    }
}

fn collect_stats(path: &Path) -> Result<(u64, u64), zb_core::Error> {
    let mut files = 0u64;
    let mut bytes = 0u64;
    let mut stack: Vec<PathBuf> = vec![path.to_path_buf()];

    while let Some(current) = stack.pop() {
        let meta = std::fs::symlink_metadata(&current).map_err(|e| zb_core::Error::FileError {
            message: format!("failed to stat {}: {e}", current.display()),
        })?;

        if meta.is_dir() {
            let entries = std::fs::read_dir(&current).map_err(|e| zb_core::Error::FileError {
                message: format!("failed to read {}: {e}", current.display()),
            })?;
            for entry in entries {
                let entry = entry.map_err(|e| zb_core::Error::FileError {
                    message: format!("failed to read entry in {}: {e}", current.display()),
                })?;
                stack.push(entry.path());
            }
        } else {
            files += 1;
            bytes += meta.len();
        }
    }

    Ok((files, bytes))
}

fn format_dep_line(deps: &BTreeSet<String>, installed: &BTreeSet<String>) -> String {
    if deps.is_empty() {
        return "(none)".to_string();
    }

    deps.iter()
        .map(|dep| {
            let mark = if installed.contains(dep) {
                style("✔").green().to_string()
            } else {
                style("✗").red().to_string()
            };
            format!("{dep} {mark}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_count(value: u64) -> String {
    let s = value.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (idx, ch) in s.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

fn format_size(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0usize;

    while size >= 1024.0 && unit < units.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes}B")
    } else {
        format!("{:.1}{}", size, units[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_count_inserts_commas() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(1_234), "1,234");
        assert_eq!(format_count(1_234_567), "1,234,567");
    }

    #[test]
    fn format_size_humanizes_bytes() {
        assert_eq!(format_size(0), "0B");
        assert_eq!(format_size(1024), "1.0KB");
        assert_eq!(format_size(1536), "1.5KB");
        assert_eq!(format_size(1024 * 1024), "1.0MB");
    }
}
