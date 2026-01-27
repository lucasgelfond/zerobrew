use regex::Regex;

use crate::entry::{BrewEntry, BrewfileEntry, RestartService};
use crate::error::BrewfileError;

#[derive(Debug)]
pub struct Brewfile {
    pub entries: Vec<BrewfileEntry>,
}

impl Brewfile {
    pub fn brew_entries(&self) -> Vec<&BrewEntry> {
        self.entries
            .iter()
            .filter_map(|e| match e {
                BrewfileEntry::Brew(brew) => Some(brew),
                _ => None,
            })
            .collect()
    }

    pub fn supported_entries(&self) -> Vec<&BrewfileEntry> {
        self.entries.iter().filter(|e| e.is_supported()).collect()
    }

    pub fn unsupported_entries(&self) -> Vec<&BrewfileEntry> {
        self.entries.iter().filter(|e| !e.is_supported()).collect()
    }
}

pub struct BrewfileParser;

impl BrewfileParser {
    pub fn parse(content: &str) -> Result<Brewfile, BrewfileError> {
        let mut entries = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let line = Self::strip_comment(line).trim();

            if line.is_empty() {
                continue;
            }

            match Self::parse_entry(line) {
                Ok(Some(entry)) => entries.push(entry),
                Ok(None) => {} // Skip lines we don't understand
                Err(e) => {
                    return Err(BrewfileError::ParseError {
                        line: line_num + 1,
                        message: e,
                    });
                }
            }
        }

        Ok(Brewfile { entries })
    }

    fn strip_comment(line: &str) -> &str {
        // Find # not inside a string
        let mut in_string = false;
        let mut escape_next = false;

        for (i, ch) in line.char_indices() {
            if escape_next {
                escape_next = false;
                continue;
            }

            match ch {
                '\\' => escape_next = true,
                '"' => in_string = !in_string,
                '#' if !in_string => return &line[..i],
                _ => {}
            }
        }

        line
    }

    fn parse_entry(line: &str) -> Result<Option<BrewfileEntry>, String> {
        // tap "name" or tap "name", "url"
        if line.starts_with("tap ") {
            return Self::parse_tap(line).map(Some);
        }

        // brew "name" with various options
        if line.starts_with("brew ") {
            return Self::parse_brew(line).map(Some);
        }

        // cask "name"
        if line.starts_with("cask ") {
            return Self::parse_cask(line).map(Some);
        }

        // mas "name", id: 123
        if line.starts_with("mas ") {
            return Self::parse_mas(line).map(Some);
        }

        // vscode "extension"
        if line.starts_with("vscode ") {
            return Self::parse_vscode(line).map(Some);
        }

        // go "package"
        if line.starts_with("go ") {
            return Self::parse_go(line).map(Some);
        }

        // cargo "crate"
        if line.starts_with("cargo ") {
            return Self::parse_cargo(line).map(Some);
        }

        // flatpak "app"
        if line.starts_with("flatpak ") {
            return Self::parse_flatpak(line).map(Some);
        }

        // cask_args (global config, skip)
        if line.starts_with("cask_args ") {
            return Ok(None);
        }

        Err(format!("unknown entry type: {}", line))
    }

    fn parse_tap(line: &str) -> Result<BrewfileEntry, String> {
        // tap "name" or tap "name", "url"
        let re = Regex::new(r#"^tap\s+"([^"]+)"(?:,\s+"([^"]+)")?"#).unwrap();

        let caps = re
            .captures(line)
            .ok_or_else(|| format!("invalid tap syntax: {}", line))?;

        Ok(BrewfileEntry::Tap {
            name: caps[1].to_string(),
            url: caps.get(2).map(|m| m.as_str().to_string()),
        })
    }

    fn parse_brew(line: &str) -> Result<BrewfileEntry, String> {
        // brew "name" or brew "name", args: [...], restart_service: ..., link: ...
        let name_re = Regex::new(r#"^brew\s+"([^"]+)"#).unwrap();

        let name = name_re
            .captures(line)
            .ok_or_else(|| format!("invalid brew syntax: {}", line))?[1]
            .to_string();

        // Parse optional arguments
        let args = Self::parse_array_option(line, "args")?;
        let restart_service = Self::parse_restart_service(line)?;
        let link = Self::parse_bool_option(line, "link")?;

        Ok(BrewfileEntry::Brew(BrewEntry {
            name,
            args,
            restart_service,
            link,
        }))
    }

    fn parse_cask(line: &str) -> Result<BrewfileEntry, String> {
        let re = Regex::new(r#"^cask\s+"([^"]+)"#).unwrap();

        let name = re
            .captures(line)
            .ok_or_else(|| format!("invalid cask syntax: {}", line))?[1]
            .to_string();

        Ok(BrewfileEntry::Cask { name })
    }

    fn parse_mas(line: &str) -> Result<BrewfileEntry, String> {
        // mas "Name", id: 123456789
        let re = Regex::new(r#"^mas\s+"([^"]+)",\s*id:\s*(\d+)"#).unwrap();

        let caps = re
            .captures(line)
            .ok_or_else(|| format!("invalid mas syntax: {}", line))?;

        Ok(BrewfileEntry::Mas {
            name: caps[1].to_string(),
            id: caps[2].parse().map_err(|_| "invalid mas id")?,
        })
    }

    fn parse_vscode(line: &str) -> Result<BrewfileEntry, String> {
        let re = Regex::new(r#"^vscode\s+"([^"]+)"#).unwrap();

        let name = re
            .captures(line)
            .ok_or_else(|| format!("invalid vscode syntax: {}", line))?[1]
            .to_string();

        Ok(BrewfileEntry::Vscode { name })
    }

    fn parse_go(line: &str) -> Result<BrewfileEntry, String> {
        let re = Regex::new(r#"^go\s+"([^"]+)"#).unwrap();

        let name = re
            .captures(line)
            .ok_or_else(|| format!("invalid go syntax: {}", line))?[1]
            .to_string();

        Ok(BrewfileEntry::Go { name })
    }

    fn parse_cargo(line: &str) -> Result<BrewfileEntry, String> {
        let re = Regex::new(r#"^cargo\s+"([^"]+)"#).unwrap();

        let name = re
            .captures(line)
            .ok_or_else(|| format!("invalid cargo syntax: {}", line))?[1]
            .to_string();

        Ok(BrewfileEntry::Cargo { name })
    }

    fn parse_flatpak(line: &str) -> Result<BrewfileEntry, String> {
        let re = Regex::new(r#"^flatpak\s+"([^"]+)"#).unwrap();

        let name = re
            .captures(line)
            .ok_or_else(|| format!("invalid flatpak syntax: {}", line))?[1]
            .to_string();

        Ok(BrewfileEntry::Flatpak {
            name,
            remote: None,
            url: None,
        })
    }

    fn parse_array_option(line: &str, option_name: &str) -> Result<Vec<String>, String> {
        let pattern = format!(r#"{}:\s*\[((?:"[^"]*"(?:,\s*)?)*)\]"#, option_name);
        let re = Regex::new(&pattern).unwrap();

        if let Some(caps) = re.captures(line) {
            let args_str = &caps[1];
            let item_re = Regex::new(r#""([^"]*)""#).unwrap();

            let items: Vec<String> = item_re
                .captures_iter(args_str)
                .map(|c| c[1].to_string())
                .collect();

            Ok(items)
        } else {
            Ok(Vec::new())
        }
    }

    fn parse_restart_service(line: &str) -> Result<Option<RestartService>, String> {
        // restart_service: true or restart_service: :changed
        let re = Regex::new(r"restart_service:\s*(:changed|true|false)").unwrap();

        if let Some(caps) = re.captures(line) {
            match &caps[1] {
                "true" => Ok(Some(RestartService::Always)),
                ":changed" => Ok(Some(RestartService::Changed)),
                "false" => Ok(None),
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    fn parse_bool_option(line: &str, option_name: &str) -> Result<Option<bool>, String> {
        let pattern = format!(r"{}:\s*(true|false)", option_name);
        let re = Regex::new(&pattern).unwrap();

        if let Some(caps) = re.captures(line) {
            match &caps[1] {
                "true" => Ok(Some(true)),
                "false" => Ok(Some(false)),
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_brew() {
        let content = r#"brew "jq""#;
        let brewfile = BrewfileParser::parse(content).unwrap();

        assert_eq!(brewfile.entries.len(), 1);
        assert!(matches!(&brewfile.entries[0], BrewfileEntry::Brew(brew) if brew.name == "jq"));
    }

    #[test]
    fn parses_brew_with_args() {
        let content = r#"brew "git", args: ["with-python", "HEAD"]"#;
        let brewfile = BrewfileParser::parse(content).unwrap();

        assert_eq!(brewfile.entries.len(), 1);
        if let BrewfileEntry::Brew(brew) = &brewfile.entries[0] {
            assert_eq!(brew.name, "git");
            assert_eq!(brew.args.len(), 2);
            assert_eq!(brew.args[0], "with-python");
        } else {
            panic!("Expected brew entry");
        }
    }

    #[test]
    fn parses_brew_with_restart_service() {
        let content = r#"brew "postgresql@15", restart_service: :changed"#;
        let brewfile = BrewfileParser::parse(content).unwrap();

        assert_eq!(brewfile.entries.len(), 1);
        if let BrewfileEntry::Brew(brew) = &brewfile.entries[0] {
            assert_eq!(brew.name, "postgresql@15");
            assert_eq!(brew.restart_service, Some(RestartService::Changed));
        } else {
            panic!("Expected brew entry");
        }
    }

    #[test]
    fn parses_tap() {
        let content = r#"tap "homebrew/cask""#;
        let brewfile = BrewfileParser::parse(content).unwrap();

        assert_eq!(brewfile.entries.len(), 1);
        assert!(
            matches!(&brewfile.entries[0], BrewfileEntry::Tap { name, .. } if name == "homebrew/cask")
        );
    }

    #[test]
    fn parses_cask() {
        let content = r#"cask "firefox""#;
        let brewfile = BrewfileParser::parse(content).unwrap();

        assert_eq!(brewfile.entries.len(), 1);
        assert!(matches!(&brewfile.entries[0], BrewfileEntry::Cask { name } if name == "firefox"));
    }

    #[test]
    fn parses_mas() {
        let content = r#"mas "Xcode", id: 497799835"#;
        let brewfile = BrewfileParser::parse(content).unwrap();

        assert_eq!(brewfile.entries.len(), 1);
        assert!(
            matches!(&brewfile.entries[0], BrewfileEntry::Mas { name, id } if name == "Xcode" && *id == 497799835)
        );
    }

    #[test]
    fn strips_comments() {
        let content = r#"
# This is a comment
brew "jq"  # inline comment
# Another comment
brew "wget"
"#;
        let brewfile = BrewfileParser::parse(content).unwrap();

        assert_eq!(brewfile.brew_entries().len(), 2);
    }

    #[test]
    fn filters_unsupported() {
        let content = r#"
tap "homebrew/core"
brew "jq"
cask "firefox"
mas "Xcode", id: 497799835
"#;
        let brewfile = BrewfileParser::parse(content).unwrap();

        assert_eq!(brewfile.supported_entries().len(), 2); // tap + brew
        assert_eq!(brewfile.unsupported_entries().len(), 2); // cask + mas
    }
}
