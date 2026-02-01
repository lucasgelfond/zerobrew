use console::style;
use std::time::{Duration, UNIX_EPOCH};

pub fn execute(
    installer: &mut zb_io::install::Installer,
    formula: String,
) -> Result<(), zb_core::Error> {
    if let Some(keg) = installer.get_installed(&formula) {
        println!("{}       {}", style("Name:").dim(), style(&keg.name).bold());
        println!("{}    {}", style("Version:").dim(), keg.version);
        println!("{}  {}", style("Store key:").dim(), &keg.store_key[..12]);
        println!(
            "{}  {}",
            style("Installed:").dim(),
            format_timestamp(keg.installed_at)
        );
    } else {
        println!("Formula '{}' is not installed.", formula);
    }

    Ok(())
}

fn format_timestamp(timestamp: i64) -> String {
    let secs = timestamp.max(0) as u64;
    let dt = UNIX_EPOCH + Duration::from_secs(secs);

    match dt.duration_since(UNIX_EPOCH) {
        Ok(dur) => format!("{}s since epoch", dur.as_secs()),
        Err(_) => "invalid timestamp".to_string(),
    }
}
