use console::style;

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
            chrono_lite_format(keg.installed_at)
        );
    } else {
        println!("Formula '{}' is not installed.", formula);
    }

    Ok(())
}

fn chrono_lite_format(timestamp: i64) -> String {
    use std::time::{Duration, UNIX_EPOCH};

    let dt = UNIX_EPOCH + Duration::from_secs(timestamp as u64);
    format!("{:?}", dt)
}
