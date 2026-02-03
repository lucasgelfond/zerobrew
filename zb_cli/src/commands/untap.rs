use console::style;
use std::path::Path;
use zb_io::db::Database;

use crate::commands::tap::parse_tap_name;

pub fn execute(root: &Path, tap: Option<String>, all: bool) -> Result<(), zb_core::Error> {
    std::fs::create_dir_all(root.join("db")).map_err(|e| zb_core::Error::StoreCorruption {
        message: format!("failed to create db directory: {e}"),
    })?;

    let db = Database::open(&root.join("db/zb.sqlite3"))?;

    if all {
        let taps = db.list_taps()?;
        if taps.is_empty() {
            println!("No taps installed.");
            return Ok(());
        }

        for tap in taps {
            let _ = db.remove_tap(&tap.owner, &tap.repo)?;
            println!(
                "{} Untapped {}/{}",
                style("==>").cyan().bold(),
                tap.owner,
                tap.repo
            );
        }

        return Ok(());
    }

    let tap = tap.ok_or(zb_core::Error::InvalidTap {
        tap: "".to_string(),
    })?;
    let (owner, repo) = parse_tap_name(&tap)?;
    let removed = db.remove_tap(&owner, &repo)?;
    if removed {
        println!("{} Untapped {}/{}", style("==>").cyan().bold(), owner, repo);
    } else {
        println!(
            "{} Tap not found: {}/{}",
            style("==>").cyan().bold(),
            owner,
            repo
        );
    }

    Ok(())
}
