//! Install a bottle's default `etc` and `var` files into the prefix.
//!
//! Bottles ship configuration under `<keg>/.bottle/etc` and `<keg>/.bottle/var`
//! (php's `php.ini`, openssl's `openssl.cnf`). Homebrew copies them into the
//! prefix when pouring rather than linking them, so users can edit them. A
//! file the user already has is never overwritten: if its contents differ,
//! the bottle's version is written next to it as `<name>.default`.

use std::fs;
use std::io;
use std::path::Path;

/// Copy `<keg>/.bottle/{etc,var}` into `<prefix>/{etc,var}`.
pub fn install_etc_var(keg_path: &Path, prefix: &Path) -> io::Result<()> {
    for dir in ["etc", "var"] {
        let src = keg_path.join(".bottle").join(dir);
        if src.is_dir() {
            copy_tree(&src, &prefix.join(dir))?;
        }
    }
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_tree(&from, &to)?;
        } else if file_type.is_symlink() {
            if to.symlink_metadata().is_err() {
                std::os::unix::fs::symlink(fs::read_link(&from)?, &to)?;
            }
        } else if to.symlink_metadata().is_err() {
            fs::copy(&from, &to)?;
        } else if to.is_file() && fs::read(&to)? != fs::read(&from)? {
            let mut default = to.into_os_string();
            default.push(".default");
            fs::copy(&from, default)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn keg_with_bottle_files(tmp: &TempDir) -> std::path::PathBuf {
        let keg = tmp.path().join("Cellar/php/8.5.11");
        let etc = keg.join(".bottle/etc/php/8.5");
        fs::create_dir_all(etc.join("php-fpm.d")).unwrap();
        fs::write(etc.join("php.ini"), "memory_limit = 128M\n").unwrap();
        fs::write(etc.join("php-fpm.d/www.conf"), "[www]\n").unwrap();
        fs::create_dir_all(keg.join(".bottle/var/log")).unwrap();
        keg
    }

    #[test]
    fn copies_etc_and_var_into_prefix() {
        let tmp = TempDir::new().unwrap();
        let keg = keg_with_bottle_files(&tmp);
        let prefix = tmp.path().join("prefix");

        install_etc_var(&keg, &prefix).unwrap();

        assert_eq!(
            fs::read_to_string(prefix.join("etc/php/8.5/php.ini")).unwrap(),
            "memory_limit = 128M\n"
        );
        assert!(prefix.join("etc/php/8.5/php-fpm.d/www.conf").is_file());
        assert!(prefix.join("var/log").is_dir());
        assert!(
            !prefix.join("etc/php/8.5/php.ini").is_symlink(),
            "config must be a real copy the user can edit"
        );
    }

    #[test]
    fn keeps_user_edits_and_writes_new_default_alongside() {
        let tmp = TempDir::new().unwrap();
        let keg = keg_with_bottle_files(&tmp);
        let prefix = tmp.path().join("prefix");
        let ini = prefix.join("etc/php/8.5/php.ini");
        fs::create_dir_all(ini.parent().unwrap()).unwrap();
        fs::write(&ini, "memory_limit = 1G\n").unwrap();

        install_etc_var(&keg, &prefix).unwrap();

        assert_eq!(fs::read_to_string(&ini).unwrap(), "memory_limit = 1G\n");
        assert_eq!(
            fs::read_to_string(prefix.join("etc/php/8.5/php.ini.default")).unwrap(),
            "memory_limit = 128M\n"
        );
    }

    #[test]
    fn identical_existing_files_are_left_as_is() {
        let tmp = TempDir::new().unwrap();
        let keg = keg_with_bottle_files(&tmp);
        let prefix = tmp.path().join("prefix");

        install_etc_var(&keg, &prefix).unwrap();
        install_etc_var(&keg, &prefix).unwrap();

        assert!(!prefix.join("etc/php/8.5/php.ini.default").exists());
    }

    #[test]
    fn preserves_file_modes() {
        let tmp = TempDir::new().unwrap();
        let keg = tmp.path().join("Cellar/openssl@3/3.6.4");
        let misc = keg.join(".bottle/etc/openssl@3/misc");
        fs::create_dir_all(&misc).unwrap();
        fs::write(misc.join("tsget"), "#!/usr/bin/perl\n").unwrap();
        fs::set_permissions(misc.join("tsget"), fs::Permissions::from_mode(0o755)).unwrap();
        let prefix = tmp.path().join("prefix");

        install_etc_var(&keg, &prefix).unwrap();

        let mode = fs::metadata(prefix.join("etc/openssl@3/misc/tsget"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn kegs_without_bottle_files_are_a_no_op() {
        let tmp = TempDir::new().unwrap();
        let keg = tmp.path().join("Cellar/jq/1.8.1");
        fs::create_dir_all(keg.join("bin")).unwrap();
        let prefix = tmp.path().join("prefix");

        install_etc_var(&keg, &prefix).unwrap();

        assert!(!prefix.join("etc").exists());
        assert!(!prefix.join("var").exists());
    }
}
