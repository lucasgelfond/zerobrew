use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "zb")]
#[command(about = "Zerobrew - A fast Homebrew-compatible package installer")]
#[command(version)]
pub struct Cli {
    #[arg(long, env = "ZEROBREW_ROOT")]
    pub root: Option<PathBuf>,

    #[arg(long, env = "ZEROBREW_PREFIX")]
    pub prefix: Option<PathBuf>,

    #[arg(
        long,
        default_value = "20",
        value_parser = parse_concurrency
    )]
    pub concurrency: usize,

    #[arg(
        long = "auto-init",
        alias = "yes",
        global = true,
        env = "ZEROBREW_AUTO_INIT"
    )]
    pub auto_init: bool,

    #[command(subcommand)]
    pub command: Commands,
}

fn parse_concurrency(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("invalid value '{}': expected a positive integer", value))?;
    if parsed == 0 {
        return Err("concurrency must be at least 1".to_string());
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn accepts_positive_concurrency() {
        let cli = Cli::try_parse_from(["zb", "--concurrency", "4", "list"]).unwrap();
        assert_eq!(cli.concurrency, 4);
    }

    #[test]
    fn rejects_zero_concurrency() {
        let result = Cli::try_parse_from(["zb", "--concurrency", "0", "list"]);
        assert!(result.is_err());
        let err = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err.contains("at least 1"));
    }

    #[test]
    fn outdated_quiet_and_verbose_conflict() {
        let result = Cli::try_parse_from(["zb", "outdated", "--quiet", "--verbose"]);
        assert!(result.is_err());
    }

    #[test]
    fn outdated_quiet_and_json_conflict() {
        let result = Cli::try_parse_from(["zb", "outdated", "--quiet", "--json"]);
        assert!(result.is_err());
    }

    #[test]
    fn outdated_verbose_and_json_conflict() {
        let result = Cli::try_parse_from(["zb", "outdated", "--verbose", "--json"]);
        assert!(result.is_err());
    }

    #[test]
    fn upgrade_parses_without_packages() {
        let cli = Cli::try_parse_from(["zb", "upgrade"]).unwrap();
        match cli.command {
            super::Commands::Upgrade {
                packages,
                build_from_source,
            } => {
                assert!(packages.is_empty());
                assert!(!build_from_source);
            }
            _ => panic!("expected upgrade command"),
        }
    }

    #[test]
    fn upgrade_parses_with_packages() {
        let cli = Cli::try_parse_from(["zb", "upgrade", "jq"]).unwrap();
        match cli.command {
            super::Commands::Upgrade {
                packages,
                build_from_source,
            } => {
                assert_eq!(packages, vec!["jq"]);
                assert!(!build_from_source);
            }
            _ => panic!("expected upgrade command"),
        }
    }

    #[test]
    fn upgrade_parses_build_from_source() {
        let cli = Cli::try_parse_from(["zb", "upgrade", "-s", "jq"]).unwrap();
        match cli.command {
            super::Commands::Upgrade {
                packages,
                build_from_source,
            } => {
                assert_eq!(packages, vec!["jq"]);
                assert!(build_from_source);
            }
            _ => panic!("expected upgrade command"),
        }
    }
}

#[derive(Subcommand)]
pub enum Commands {
    Install {
        #[arg(required = true, num_args = 1..)]
        formulas: Vec<String>,
        #[arg(long)]
        no_link: bool,
        #[arg(long, short = 's')]
        build_from_source: bool,
    },
    Bundle {
        #[command(subcommand)]
        command: Option<BundleCommands>,
    },
    Uninstall {
        #[arg(required_unless_present = "all", num_args = 1..)]
        formulas: Vec<String>,
        #[arg(long)]
        all: bool,
    },
    Migrate {
        #[arg(long, short = 'y')]
        yes: bool,
        #[arg(long)]
        force: bool,
    },
    List,
    Info {
        formula: String,
    },
    Gc,
    Reset {
        #[arg(long, short = 'y')]
        yes: bool,
    },
    Init {
        #[arg(long)]
        no_modify_path: bool,
    },
    Completion {
        #[arg(value_enum)]
        shell: clap_complete::shells::Shell,
    },
    #[command(disable_help_flag = true)]
    Run {
        formula: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Update,
    Upgrade {
        #[arg(num_args = 0..)]
        packages: Vec<String>,
        #[arg(long, short = 's')]
        build_from_source: bool,
    },
    Outdated {
        /// Show only package names
        #[arg(short, long, conflicts_with_all = ["verbose", "json"])]
        quiet: bool,

        /// Show detailed version information
        #[arg(short, long, conflicts_with_all = ["quiet", "json"])]
        verbose: bool,

        /// Output as JSON
        #[arg(long, conflicts_with_all = ["quiet", "verbose"])]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum BundleCommands {
    Install {
        #[arg(long, short = 'f', value_name = "FILE", default_value = "Brewfile")]
        file: PathBuf,
        #[arg(long)]
        no_link: bool,
    },
    Dump {
        #[arg(long, short = 'f', value_name = "FILE", default_value = "Brewfile")]
        file: PathBuf,
        #[arg(long)]
        force: bool,
    },
}
