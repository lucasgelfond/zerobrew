use console::style;
use std::collections::HashSet;
use std::path::Path;
use std::time::Instant;

use super::install;

pub async fn execute(
    installer: &mut zb_io::Installer,
    manifest_path: &Path,
    no_link: bool,
) -> Result<(), zb_core::Error> {
    let formulas = load_manifest(manifest_path)?;
    println!(
        "{} Installing {} formulas from {}...",
        style("==>").cyan().bold(),
        style(formulas.len()).green().bold(),
        manifest_path.display()
    );

    let start = Instant::now();
    for formula in formulas {
        install::execute(installer, vec![formula], no_link).await?;
    }

    println!(
        "{} Finished installing manifest in {:.2}s",
        style("==>").cyan().bold(),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn load_manifest(path: &Path) -> Result<Vec<String>, zb_core::Error> {
    let contents = std::fs::read_to_string(path).map_err(|e| zb_core::Error::FileError {
        message: format!("failed to read manifest {}: {}", path.display(), e),
    })?;

    let mut formulas = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for line in contents.lines() {
        // Handle inline comments by splitting on '#' and taking the first part
        let entry = line.split('#').next().unwrap_or("").trim();
        if entry.is_empty() {
            continue;
        }

        if seen.insert(entry.to_string()) {
            formulas.push(entry.to_string());
        }
    }

    if formulas.is_empty() {
        return Err(zb_core::Error::FileError {
            message: format!("manifest {} did not contain any formulas", path.display()),
        });
    }

    Ok(formulas)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_manifest_parses_entries_ignoring_whitespace_and_comments() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            file,
            "# comment\n\njq\nwget\njq\n   git  \n# another comment"
        )
        .unwrap();

        let entries = load_manifest(file.path()).unwrap();
        assert_eq!(entries, vec!["jq", "wget", "git"]);
    }

    #[test]
    fn load_manifest_handles_inline_comments() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            file,
            "jq # inline comment\nwget# no space\n  git  # with spaces  "
        )
        .unwrap();

        let entries = load_manifest(file.path()).unwrap();
        assert_eq!(entries, vec!["jq", "wget", "git"]);
    }

    #[test]
    fn load_manifest_errors_when_only_comments() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "# nothing here\n   # still nothing").unwrap();

        let err = load_manifest(file.path()).unwrap_err();
        match err {
            zb_core::Error::FileError { message } => {
                assert!(message.contains("did not contain any formulas"))
            }
            other => panic!("expected file error, got {other:?}"),
        }
    }

    #[test]
    fn load_manifest_errors_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");

        let err = load_manifest(&missing).unwrap_err();
        match err {
            zb_core::Error::FileError { message } => {
                assert!(message.contains("failed to read manifest"))
            }
            other => panic!("expected file error, got {other:?}"),
        }
    }
}
