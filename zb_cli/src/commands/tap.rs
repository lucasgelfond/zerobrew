use console::style;
use std::path::Path;
use zb_io::db::Database;

pub fn parse_tap_name(tap: &str) -> Result<(String, String), zb_core::Error> {
    let parts: Vec<&str> = tap.split('/').collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        return Ok((parts[0].to_string(), parts[1].to_string()));
    }

    Err(zb_core::Error::InvalidTap {
        tap: tap.to_string(),
    })
}

pub fn execute(root: &Path, tap: Option<String>, full: bool) -> Result<(), zb_core::Error> {
    std::fs::create_dir_all(root.join("db")).map_err(|e| zb_core::Error::StoreCorruption {
        message: format!("failed to create db directory: {e}"),
    })?;

    let db = Database::open(&root.join("db/zb.sqlite3"))?;

    if let Some(tap) = tap {
        let (owner, repo) = parse_tap_name(&tap)?;
        let added = db.add_tap(&owner, &repo)?;
        if added {
            println!("{} Tapped {}/{}", style("==>").cyan().bold(), owner, repo);
        } else {
            println!(
                "{} Already tapped {}/{}",
                style("==>").cyan().bold(),
                owner,
                repo
            );
        }
        return Ok(());
    }

    let taps = db.list_taps()?;
    if taps.is_empty() {
        println!("No taps installed.");
        return Ok(());
    }

    if full {
        for tap in taps {
            println!(
                "{}/{} priority={} added_at={}",
                tap.owner, tap.repo, tap.priority, tap.added_at
            );
        }
    } else {
        for tap in taps {
            println!("{}/{}", tap.owner, tap.repo);
        }
    }

    Ok(())
}
