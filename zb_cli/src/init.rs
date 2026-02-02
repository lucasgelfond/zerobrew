use console::style;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
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

pub fn run_init(root: &Path, prefix: &Path, no_modify_path: bool) -> Result<(), InitError> {
    println!("{} Initializing zerobrew...", style("==>").cyan().bold());

    let zerobrew_dir = match std::env::var("ZEROBREW_DIR") {
        Ok(dir) => dir,
        Err(_) => {
            let home = std::env::var("HOME")
                .map_err(|_| InitError::Message("HOME not set".to_string()))?;
            format!("{}/.zerobrew", home)
        }
    };
    let zerobrew_bin = format!("{}/bin", zerobrew_dir);

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

    add_to_path(prefix, &zerobrew_dir, &zerobrew_bin, root, no_modify_path)?;

    println!("{} Initialization complete!", style("==>").cyan().bold());

    Ok(())
}

fn add_to_path(
    prefix: &Path,
    zerobrew_dir: &str,
    zerobrew_bin: &str,
    root: &Path,
    no_modify_path: bool,
) -> Result<(), InitError> {
    let shell = std::env::var("SHELL").unwrap_or_default();
    let home = std::env::var("HOME").map_err(|_| InitError::Message("HOME not set".to_string()))?;

    let config_file = if shell.contains("zsh") {
        let zdotdir = std::env::var("ZDOTDIR").unwrap_or_else(|_| home.clone());
        let zshenv = format!("{}/.zshenv", zdotdir);

        if std::path::Path::new(&zshenv).exists() {
            zshenv
        } else {
            format!("{}/.zshrc", zdotdir)
        }
    } else if shell.contains("bash") {
        let bash_profile = format!("{}/.bash_profile", home);
        if std::path::Path::new(&bash_profile).exists() {
            bash_profile
        } else {
            format!("{}/.bashrc", home)
        }
    } else {
        format!("{}/.profile", home)
    };

    let prefix_bin = prefix.join("bin");

    // Check if zerobrew is already configured
    let already_added = if let Ok(contents) = std::fs::read_to_string(&config_file) {
        contents.contains("# zerobrew")
    } else {
        false
    };

    if !no_modify_path && !already_added {
        // Build the shell configuration content
        let config_content = format!(
            "\n# zerobrew
export ZEROBREW_DIR={}
export ZEROBREW_BIN={}
export ZEROBREW_ROOT={}
export ZEROBREW_PREFIX={}
export PKG_CONFIG_PATH=\"{}/lib/pkgconfig:${{PKG_CONFIG_PATH:-}}\"
_zb_path_append() {{
    local argpath=\"$1\"
    case \":${{PATH}}:\" in
        *:\"$argpath\":*) ;;
        *) export PATH=\"$argpath:$PATH\" ;;
    esac;
}}
_zb_path_append {}
_zb_path_append {}
",
            zerobrew_dir,
            zerobrew_bin,
            root.display(),
            prefix.display(),
            prefix.display(),
            zerobrew_bin,
            prefix_bin.display()
        );

        let write_result = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config_file)
            .and_then(|mut f| f.write_all(config_content.as_bytes()));

        if let Err(e) = write_result {
            println!(
                "{} Could not write to {} due to error: {}",
                style("Warning:").yellow().bold(),
                config_file,
                e
            );
            println!(
                "{} Please add the following to {}:",
                style("Info:").cyan().bold(),
                config_file
            );
            println!("{}", config_content);
        } else {
            println!(
                "    {} Added zerobrew configuration to {}",
                style("✓").green(),
                config_file
            );
            println!(
                "    {} Added {} and {} to PATH",
                style("✓").green(),
                zerobrew_bin,
                prefix_bin.display()
            );
        }
    } else if no_modify_path {
        println!(
            "    {} Skipped shell configuration (--no-modify-path)",
            style("→").cyan()
        );
        println!(
            "    {} To use zerobrew, add {} and {} to your PATH",
            style("→").cyan(),
            zerobrew_bin,
            prefix_bin.display()
        );
    }

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

    // Pass false for no_modify_shell since user confirmed they want full initialization
    run_init(root, prefix, false).map_err(|e| match e {
        InitError::Message(msg) => zb_core::Error::StoreCorruption { message: msg },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    #[test]
    fn needs_init_when_directories_missing() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("nonexistent_root");
        let prefix = tmp.path().join("nonexistent_prefix");

        assert!(needs_init(&root, &prefix));
    }

    #[test]
    fn needs_init_when_not_writable() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("root");
        let prefix = tmp.path().join("prefix");

        fs::create_dir(&root).unwrap();
        fs::create_dir(&prefix).unwrap();

        // Make directories read-only
        let mut root_perms = fs::metadata(&root).unwrap().permissions();
        root_perms.set_mode(0o555);
        fs::set_permissions(&root, root_perms).unwrap();

        let result = needs_init(&root, &prefix);

        // Restore permissions for cleanup
        let mut root_perms = fs::metadata(&root).unwrap().permissions();
        root_perms.set_mode(0o755);
        fs::set_permissions(&root, root_perms).unwrap();

        assert!(result);
    }

    #[test]
    fn no_init_needed_when_writable() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("root");
        let prefix = tmp.path().join("prefix");

        fs::create_dir(&root).unwrap();
        fs::create_dir(&prefix).unwrap();

        assert!(!needs_init(&root, &prefix));
    }

    #[test]
    fn is_writable_returns_true_for_writable_dir() {
        let tmp = TempDir::new().unwrap();
        assert!(is_writable(tmp.path()));
    }

    #[test]
    fn is_writable_returns_false_for_nonexistent_path() {
        let tmp = TempDir::new().unwrap();
        let nonexistent = tmp.path().join("does_not_exist");
        assert!(!is_writable(&nonexistent));
    }

    #[test]
    fn is_writable_returns_false_for_readonly_dir() {
        let tmp = TempDir::new().unwrap();
        let readonly = tmp.path().join("readonly");
        fs::create_dir(&readonly).unwrap();

        let mut perms = fs::metadata(&readonly).unwrap().permissions();
        perms.set_mode(0o555);
        fs::set_permissions(&readonly, perms).unwrap();

        assert!(!is_writable(&readonly));

        // Restore permissions for cleanup
        let mut perms = fs::metadata(&readonly).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&readonly, perms).unwrap();
    }

    #[test]
    fn add_to_path_writes_all_env_vars() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let prefix = tmp.path().join("prefix");
        let root = tmp.path().join("root");
        let shell_config = home.join(".bashrc");
        let zerobrew_dir = "/home/user/.zerobrew";
        let zerobrew_bin = "/home/user/.zerobrew/bin";

        fs::create_dir(&prefix).unwrap();
        fs::create_dir(&root).unwrap();

        // Set up environment to simulate bash
        unsafe {
            std::env::set_var("HOME", home.to_str().unwrap());
        }
        unsafe {
            std::env::set_var("SHELL", "/bin/bash");
        }

        add_to_path(&prefix, zerobrew_dir, zerobrew_bin, &root, false).unwrap();

        let content = fs::read_to_string(&shell_config).unwrap();
        assert!(content.contains("export ZEROBREW_DIR=/home/user/.zerobrew"));
        assert!(content.contains("export ZEROBREW_BIN=/home/user/.zerobrew/bin"));
        assert!(content.contains(&format!("export ZEROBREW_ROOT={}", root.display())));
        assert!(content.contains(&format!("export ZEROBREW_PREFIX={}", prefix.display())));
        assert!(content.contains("export PKG_CONFIG_PATH="));
        assert!(content.contains("/lib/pkgconfig"));
    }

    #[test]
    fn add_to_path_includes_path_append_function() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let prefix = tmp.path().join("prefix");
        let root = tmp.path().join("root");
        let shell_config = home.join(".bashrc");
        let zerobrew_dir = "/home/user/.zerobrew";
        let zerobrew_bin = "/home/user/.zerobrew/bin";

        fs::create_dir(&prefix).unwrap();
        fs::create_dir(&root).unwrap();

        unsafe {
            std::env::set_var("HOME", home.to_str().unwrap());
        }
        unsafe {
            std::env::set_var("SHELL", "/bin/bash");
        }

        add_to_path(&prefix, zerobrew_dir, zerobrew_bin, &root, false).unwrap();

        let content = fs::read_to_string(&shell_config).unwrap();
        assert!(content.contains("_zb_path_append()"));
        assert!(content.contains("case \":${PATH}:"));
        assert!(content.contains("_zb_path_append"));
    }

    #[test]
    fn add_to_path_adds_both_paths() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let prefix = tmp.path().join("prefix");
        let root = tmp.path().join("root");
        let shell_config = home.join(".bashrc");
        let zerobrew_dir = "/home/user/.zerobrew";
        let zerobrew_bin = "/home/user/.zerobrew/bin";

        fs::create_dir(&prefix).unwrap();
        fs::create_dir(&root).unwrap();

        unsafe {
            std::env::set_var("HOME", home.to_str().unwrap());
        }
        unsafe {
            std::env::set_var("SHELL", "/bin/bash");
        }

        add_to_path(&prefix, zerobrew_dir, zerobrew_bin, &root, false).unwrap();

        let content = fs::read_to_string(&shell_config).unwrap();
        // Both paths should be added via _zb_path_append
        assert!(content.contains("_zb_path_append /home/user/.zerobrew/bin"));
        assert!(content.contains(&format!("_zb_path_append {}", prefix.join("bin").display())));
    }

    #[test]
    fn add_to_path_no_modify_shell_skips_write() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let prefix = tmp.path().join("prefix");
        let root = tmp.path().join("root");
        let shell_config = home.join(".bashrc");
        let zerobrew_dir = "/home/user/.zerobrew";
        let zerobrew_bin = "/home/user/.zerobrew/bin";

        fs::create_dir(&prefix).unwrap();
        fs::create_dir(&root).unwrap();

        unsafe {
            std::env::set_var("HOME", home.to_str().unwrap());
        }
        unsafe {
            std::env::set_var("SHELL", "/bin/bash");
        }

        add_to_path(&prefix, zerobrew_dir, zerobrew_bin, &root, true).unwrap();

        // File should not be created
        assert!(!shell_config.exists());
    }

    #[test]
    fn add_to_path_no_duplicate_config() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let prefix = tmp.path().join("prefix");
        let root = tmp.path().join("root");
        let shell_config = home.join(".bashrc");
        let zerobrew_dir = "/home/user/.zerobrew";
        let zerobrew_bin = "/home/user/.zerobrew/bin";

        fs::create_dir(&prefix).unwrap();
        fs::create_dir(&root).unwrap();

        unsafe {
            std::env::set_var("HOME", home.to_str().unwrap());
        }
        unsafe {
            std::env::set_var("SHELL", "/bin/bash");
        }

        // Write initial config
        fs::write(&shell_config, "# existing content\n# zerobrew\n").unwrap();

        add_to_path(&prefix, zerobrew_dir, zerobrew_bin, &root, false).unwrap();

        // Content should remain unchanged since # zerobrew already exists
        let content = fs::read_to_string(&shell_config).unwrap();
        assert!(!content.contains("export ZEROBREW_DIR"));
        assert_eq!(content, "# existing content\n# zerobrew\n");
    }

    #[test]
    fn add_to_path_uses_zshrc_for_zsh() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let prefix = tmp.path().join("prefix");
        let root = tmp.path().join("root");
        let shell_config = home.join(".zshrc");
        let zerobrew_dir = "/home/user/.zerobrew";
        let zerobrew_bin = "/home/user/.zerobrew/bin";

        fs::create_dir(&prefix).unwrap();
        fs::create_dir(&root).unwrap();

        unsafe {
            std::env::set_var("HOME", home.to_str().unwrap());
            std::env::set_var("SHELL", "/bin/zsh");
            std::env::remove_var("ZDOTDIR");
        }

        add_to_path(&prefix, zerobrew_dir, zerobrew_bin, &root, false).unwrap();

        assert!(shell_config.exists());
        let content = fs::read_to_string(&shell_config).unwrap();
        assert!(content.contains("# zerobrew"));
    }

    #[test]
    fn add_to_path_prefers_zshenv_when_exists() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let prefix = tmp.path().join("prefix");
        let root = tmp.path().join("root");
        let zshenv = home.join(".zshenv");
        let zshrc = home.join(".zshrc");
        let zerobrew_dir = "/home/user/.zerobrew";
        let zerobrew_bin = "/home/user/.zerobrew/bin";

        fs::create_dir(&prefix).unwrap();
        fs::create_dir(&root).unwrap();

        // Create .zshenv first
        fs::write(&zshenv, "# existing zshenv\n").unwrap();

        unsafe {
            std::env::set_var("HOME", home.to_str().unwrap());
        }
        unsafe {
            std::env::set_var("SHELL", "/bin/zsh");
        }
        unsafe {
            std::env::remove_var("ZDOTDIR");
        }

        add_to_path(&prefix, zerobrew_dir, zerobrew_bin, &root, false).unwrap();

        // Should write to .zshenv, not .zshrc
        assert!(zshenv.exists());
        let zshenv_content = fs::read_to_string(&zshenv).unwrap();
        assert!(zshenv_content.contains("# zerobrew"));
        assert!(!zshrc.exists());
    }

    #[test]
    fn add_to_path_uses_bash_profile_when_exists() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let prefix = tmp.path().join("prefix");
        let root = tmp.path().join("root");
        let bash_profile = home.join(".bash_profile");
        let bashrc = home.join(".bashrc");
        let zerobrew_dir = "/home/user/.zerobrew";
        let zerobrew_bin = "/home/user/.zerobrew/bin";

        fs::create_dir(&prefix).unwrap();
        fs::create_dir(&root).unwrap();

        // Create .bash_profile first
        fs::write(&bash_profile, "# existing bash_profile\n").unwrap();

        unsafe {
            std::env::set_var("HOME", home.to_str().unwrap());
        }
        unsafe {
            std::env::set_var("SHELL", "/bin/bash");
        }

        add_to_path(&prefix, zerobrew_dir, zerobrew_bin, &root, false).unwrap();

        // Should write to .bash_profile, not .bashrc
        assert!(bash_profile.exists());
        let profile_content = fs::read_to_string(&bash_profile).unwrap();
        assert!(profile_content.contains("# zerobrew"));
        assert!(!bashrc.exists());
    }

    #[test]
    fn add_to_path_uses_profile_for_other_shells() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let prefix = tmp.path().join("prefix");
        let root = tmp.path().join("root");
        let profile = home.join(".profile");
        let zerobrew_dir = "/home/user/.zerobrew";
        let zerobrew_bin = "/home/user/.zerobrew/bin";

        fs::create_dir(&prefix).unwrap();
        fs::create_dir(&root).unwrap();

        unsafe {
            std::env::set_var("HOME", home.to_str().unwrap());
        }
        unsafe {
            std::env::set_var("SHELL", "/bin/fish");
        }

        add_to_path(&prefix, zerobrew_dir, zerobrew_bin, &root, false).unwrap();

        assert!(profile.exists());
        let content = fs::read_to_string(&profile).unwrap();
        assert!(content.contains("# zerobrew"));
    }

    #[test]
    fn add_to_path_uses_zdotdir_when_set() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let zdotdir = tmp.path().join("zsh_config");
        let prefix = tmp.path().join("prefix");
        let root = tmp.path().join("root");
        let shell_config = zdotdir.join(".zshrc");
        let zerobrew_dir = "/home/user/.zerobrew";
        let zerobrew_bin = "/home/user/.zerobrew/bin";

        fs::create_dir(&zdotdir).unwrap();
        fs::create_dir(&prefix).unwrap();
        fs::create_dir(&root).unwrap();

        unsafe {
            std::env::set_var("HOME", home.to_str().unwrap());
        }
        unsafe {
            std::env::set_var("SHELL", "/bin/zsh");
        }
        unsafe {
            std::env::set_var("ZDOTDIR", zdotdir.to_str().unwrap());
        }

        add_to_path(&prefix, zerobrew_dir, zerobrew_bin, &root, false).unwrap();

        // Should write to $ZDOTDIR/.zshrc
        assert!(shell_config.exists());
        let content = fs::read_to_string(&shell_config).unwrap();
        assert!(content.contains("# zerobrew"));
    }
}
