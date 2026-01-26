use std::path::Path;
use std::process::Command;
use zb_core::Error;

/// Entry parsed from a Brewfile
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrewfileEntry {
    /// A formula to install via `brew "name"`
    Formula(String),
    /// A cask (not supported yet)
    Cask(String),
    /// A tap (not supported yet)
    Tap(String),
}

/// Parsed Brewfile contents
#[derive(Debug, Default)]
pub struct Brewfile {
    pub entries: Vec<BrewfileEntry>,
}

impl Brewfile {
    /// Parse a Brewfile by evaluating it with Ruby
    ///
    /// This provides full compatibility with Brewfile DSL including
    /// conditionals, loops, and other Ruby features.
    pub fn parse(path: &Path) -> Result<Self, Error> {
        if !path.exists() {
            return Err(Error::StoreCorruption {
                message: format!("Brewfile not found: {}", path.display()),
            });
        }

        // Ruby script that evaluates the Brewfile DSL and outputs entries
        let evaluator = r#"
# Brewfile DSL evaluator for zerobrew
$entries = []

def brew(name, **opts)
  $entries << "brew:#{name}"
end

def cask(name, **opts)
  $entries << "cask:#{name}"
end

def tap(name, **opts)
  $entries << "tap:#{name}"
end

def mas(name, **opts)
  # Mac App Store - not supported, silently ignore
end

def vscode(name, **opts)
  # VS Code extensions - not supported, silently ignore
end

def whalebrew(name, **opts)
  # Whalebrew - not supported, silently ignore
end

# Global configuration methods - ignore but don't error
def cask_args(**opts); end
def brew_args(**opts); end

# Catch-all for any other undefined Brewfile DSL methods
def self.method_missing(method, *args, **kwargs, &block)
  # Silently ignore unknown methods to be forward-compatible
  # with new Brewfile features
end

# Load and evaluate the Brewfile
begin
  load ARGV[0]
rescue => e
  $stderr.puts "Brewfile error: #{e.message}"
  exit 1
end

# Output entries
$entries.each { |e| puts e }
"#;

        let output = Command::new("ruby")
            .args(["-e", evaluator, &path.to_string_lossy()])
            .output()
            .map_err(|e| Error::StoreCorruption {
                message: format!("Failed to run Ruby: {}", e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::StoreCorruption {
                message: format!("Brewfile evaluation failed: {}", stderr.trim()),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut entries = Vec::new();

        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(name) = line.strip_prefix("brew:") {
                entries.push(BrewfileEntry::Formula(name.to_string()));
            } else if let Some(name) = line.strip_prefix("cask:") {
                entries.push(BrewfileEntry::Cask(name.to_string()));
            } else if let Some(name) = line.strip_prefix("tap:") {
                entries.push(BrewfileEntry::Tap(name.to_string()));
            }
        }

        Ok(Brewfile { entries })
    }

    /// Get only the core formula names (what zerobrew can install)
    /// Excludes tap formulas (those with `/` in the name like `user/repo/formula`)
    pub fn formulas(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|e| match e {
                BrewfileEntry::Formula(name) if !name.contains('/') => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    /// Get tap formula names (formulas from third-party taps, not supported yet)
    pub fn tap_formulas(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|e| match e {
                BrewfileEntry::Formula(name) if name.contains('/') => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    /// Get cask names (for warning about unsupported entries)
    pub fn casks(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|e| match e {
                BrewfileEntry::Cask(name) => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    /// Get tap names (for warning about unsupported entries)
    pub fn taps(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|e| match e {
                BrewfileEntry::Tap(name) => Some(name.clone()),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn parses_simple_brewfile() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"brew "wget""#).unwrap();
        writeln!(file, r#"brew "jq""#).unwrap();
        writeln!(file, r#"brew "ripgrep""#).unwrap();

        let brewfile = Brewfile::parse(file.path()).unwrap();
        let formulas = brewfile.formulas();

        assert_eq!(formulas, vec!["wget", "jq", "ripgrep"]);
    }

    #[test]
    fn parses_brewfile_with_casks_and_taps() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"tap "homebrew/cask""#).unwrap();
        writeln!(file, r#"brew "git""#).unwrap();
        writeln!(file, r#"cask "visual-studio-code""#).unwrap();

        let brewfile = Brewfile::parse(file.path()).unwrap();

        assert_eq!(brewfile.formulas(), vec!["git"]);
        assert_eq!(brewfile.casks(), vec!["visual-studio-code"]);
        assert_eq!(brewfile.taps(), vec!["homebrew/cask"]);
    }

    #[test]
    fn handles_brew_with_options() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"brew "vim", args: ["with-lua"]"#).unwrap();
        writeln!(file, r#"brew "emacs", restart_service: true"#).unwrap();

        let brewfile = Brewfile::parse(file.path()).unwrap();
        let formulas = brewfile.formulas();

        assert_eq!(formulas, vec!["vim", "emacs"]);
    }

    #[test]
    fn handles_conditionals() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"brew "coreutils""#).unwrap();
        writeln!(file, r#"if RUBY_PLATFORM.include?("darwin")"#).unwrap();
        writeln!(file, r#"  brew "macos-only""#).unwrap();
        writeln!(file, r#"end"#).unwrap();

        let brewfile = Brewfile::parse(file.path()).unwrap();
        let formulas = brewfile.formulas();

        // On macOS, both should be present
        assert!(formulas.contains(&"coreutils".to_string()));
        #[cfg(target_os = "macos")]
        assert!(formulas.contains(&"macos-only".to_string()));
    }

    #[test]
    fn returns_error_for_missing_file() {
        let result = Brewfile::parse(Path::new("/nonexistent/Brewfile"));
        assert!(result.is_err());
    }

    #[test]
    fn returns_error_for_syntax_error() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "this is not valid ruby {{{{").unwrap();

        let result = Brewfile::parse(file.path());
        assert!(result.is_err());
    }

    #[test]
    fn handles_cask_args_and_unknown_methods() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"cask_args appdir: '/Applications', require_sha: true"#
        )
        .unwrap();
        writeln!(file, r#"some_future_method "whatever""#).unwrap();
        writeln!(file, r#"brew "wget""#).unwrap();

        let brewfile = Brewfile::parse(file.path()).unwrap();
        let formulas = brewfile.formulas();

        assert_eq!(formulas, vec!["wget"]);
    }
}
