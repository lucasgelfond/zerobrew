use crate::entry::{BrewEntry, BrewfileEntry, RestartService};
use crate::error::BrewfileError;
use crate::parser::Brewfile;

use zb_io::Installer;

pub struct Exporter<'a> {
    installer: &'a Installer,
}

impl<'a> Exporter<'a> {
    pub fn new(installer: &'a Installer) -> Self {
        Self { installer }
    }

    pub fn export(&self) -> Result<Brewfile, BrewfileError> {
        let kegs = self.installer.list_installed()?;

        // Convert to brew entries
        // Note: We don't have a way to track user-requested vs dependency yet
        // So we export everything for now
        let entries: Vec<BrewfileEntry> = kegs
            .iter()
            .map(|keg| {
                BrewfileEntry::Brew(BrewEntry {
                    name: keg.name.clone(),
                    args: Vec::new(),
                    restart_service: None,
                    link: Some(true),
                })
            })
            .collect();

        Ok(Brewfile { entries })
    }

    pub fn to_string(&self) -> Result<String, BrewfileError> {
        let brewfile = self.export()?;
        Ok(Self::format_brewfile(&brewfile))
    }

    pub fn format_brewfile(brewfile: &Brewfile) -> String {
        let mut output = String::new();

        // Group entries by type
        let taps: Vec<_> = brewfile
            .entries
            .iter()
            .filter_map(|e| match e {
                BrewfileEntry::Tap { name, url } => Some((name, url)),
                _ => None,
            })
            .collect();

        let brews: Vec<_> = brewfile
            .entries
            .iter()
            .filter_map(|e| match e {
                BrewfileEntry::Brew(brew) => Some(brew),
                _ => None,
            })
            .collect();

        // Write taps first
        for (name, url) in &taps {
            if let Some(u) = url {
                output.push_str(&format!("tap \"{}\", \"{}\"\n", name, u));
            } else {
                output.push_str(&format!("tap \"{}\"\n", name));
            }
        }

        if !taps.is_empty() && !brews.is_empty() {
            output.push('\n');
        }

        // Write brews
        for brew in brews {
            if brew.args.is_empty() && brew.restart_service.is_none() && brew.link != Some(false) {
                // Simple case
                output.push_str(&format!("brew \"{}\"\n", brew.name));
            } else {
                // Complex case with options
                let mut parts = vec![format!("brew \"{}\"", brew.name)];

                if !brew.args.is_empty() {
                    let args_str = brew
                        .args
                        .iter()
                        .map(|a| format!("\"{}\"", a))
                        .collect::<Vec<_>>()
                        .join(", ");
                    parts.push(format!("args: [{}]", args_str));
                }

                if let Some(restart) = brew.restart_service {
                    let restart_str = match restart {
                        RestartService::Always => "true",
                        RestartService::Changed => ":changed",
                    };
                    parts.push(format!("restart_service: {}", restart_str));
                }

                if brew.link == Some(false) {
                    parts.push("link: false".to_string());
                }

                output.push_str(&parts.join(", "));
                output.push('\n');
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::RestartService;

    #[test]
    fn formats_simple_brewfile() {
        let entries = vec![BrewfileEntry::Brew(BrewEntry {
            name: "jq".to_string(),
            args: Vec::new(),
            restart_service: None,
            link: None,
        })];

        let brewfile = Brewfile { entries };
        let output = Exporter::format_brewfile(&brewfile);

        assert_eq!(output, "brew \"jq\"\n");
    }

    #[test]
    fn formats_brew_with_options() {
        let entries = vec![BrewfileEntry::Brew(BrewEntry {
            name: "git".to_string(),
            args: vec!["with-python".to_string()],
            restart_service: None,
            link: Some(false),
        })];

        let brewfile = Brewfile { entries };
        let output = Exporter::format_brewfile(&brewfile);

        assert!(output.contains("brew \"git\""));
        assert!(output.contains("args: [\"with-python\"]"));
        assert!(output.contains("link: false"));
    }

    #[test]
    fn formats_brew_with_service_hint() {
        let entries = vec![BrewfileEntry::Brew(BrewEntry {
            name: "postgresql@15".to_string(),
            args: Vec::new(),
            restart_service: Some(RestartService::Changed),
            link: None,
        })];

        let brewfile = Brewfile { entries };
        let output = Exporter::format_brewfile(&brewfile);

        assert!(output.contains("brew \"postgresql@15\""));
        assert!(output.contains("restart_service: :changed"));
    }
}
