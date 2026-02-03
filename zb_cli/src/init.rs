use console::style;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

pub enum InitError {
    Message(String),
}

pub fn needs_init(root: &Path, prefix: &Path) -> bool {
    let root_ok = root.exists() && is_writable(root);
    let prefix_ok = prefix.exists() && is_writable(prefix);
    !(root_ok && prefix_ok)
}

pub fn is_writable(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let test_file = path.join(".zb_write_test");
    match std::fs::write(&test_file, b"test") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            true
        }
        Err(_) => false,
    }
}

pub fn run_init(root: &Path, prefix: &Path) -> Result<(), InitError> {
    println!("{} Initializing zerobrew...", style("==>").cyan().bold());

    let dirs_to_create: Vec<PathBuf> = vec![
        root.to_path_buf(),
        root.join("store"),
        root.join("db"),
        root.join("cache"),
        root.join("locks"),
        prefix.to_path_buf(),
        prefix.join("bin"),
        prefix.join("Cellar"),
    ];

    let need_sudo = dirs_to_create.iter().any(|d| {
        if d.exists() {
            !is_writable(d)
        } else {
            d.parent()
                .map(|p| p.exists() && !is_writable(p))
                .unwrap_or(true)
        }
    });

    if need_sudo {
        println!(
            "{}",
            style("    Creating directories (requires sudo)...").dim()
        );

        for dir in &dirs_to_create {
            let status = Command::new("sudo")
                .args(["mkdir", "-p", &dir.to_string_lossy()])
                .status()
                .map_err(|e| InitError::Message(format!("Failed to run sudo mkdir: {}", e)))?;

            if !status.success() {
                return Err(InitError::Message(format!(
                    "Failed to create directory: {}",
                    dir.display()
                )));
            }
        }

        let user = Command::new("whoami")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "root".to_string()));

        let status = Command::new("sudo")
            .args(["chown", "-R", &user, &root.to_string_lossy()])
            .status()
            .map_err(|e| InitError::Message(format!("Failed to run sudo chown: {}", e)))?;

        if !status.success() {
            return Err(InitError::Message(format!(
                "Failed to set ownership on {}",
                root.display()
            )));
        }

        let status = Command::new("sudo")
            .args(["chown", "-R", &user, &prefix.to_string_lossy()])
            .status()
            .map_err(|e| InitError::Message(format!("Failed to run sudo chown: {}", e)))?;

        if !status.success() {
            return Err(InitError::Message(format!(
                "Failed to set ownership on {}",
                prefix.display()
            )));
        }
    } else {
        for dir in &dirs_to_create {
            std::fs::create_dir_all(dir).map_err(|e| {
                InitError::Message(format!("Failed to create {}: {}", dir.display(), e))
            })?;
        }
    }

    let bin_path = prefix.join("bin");
    let current_path = std::env::var("PATH").unwrap_or_default();
    if !current_path.contains(&bin_path.to_string_lossy().to_string()) {
        println!(
            "    {} Add {} to your PATH, or re-run the installer",
            style("→").cyan(),
            style(bin_path.display()).cyan()
        );
    }

    println!("{} Initialization complete!", style("==>").cyan().bold());

    Ok(())
}

pub fn ensure_init(root: &Path, prefix: &Path) -> Result<(), zb_core::Error> {
    if !needs_init(root, prefix) {
        return Ok(());
    }

    println!(
        "{} Zerobrew needs to be initialized first.",
        style("Note:").yellow().bold()
    );
    println!("    This will create directories at:");
    println!("      • {}", root.display());
    println!("      • {}", prefix.display());
    println!();

    print!("Initialize now? [Y/n] ");
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let input = input.trim();

    if !input.is_empty() && !input.eq_ignore_ascii_case("y") && !input.eq_ignore_ascii_case("yes") {
        return Err(zb_core::Error::StoreCorruption {
            message: "Initialization required. Run 'zb init' first.".to_string(),
        });
    }

    run_init(root, prefix).map_err(|e| match e {
        InitError::Message(msg) => zb_core::Error::StoreCorruption { message: msg },
    })
}
