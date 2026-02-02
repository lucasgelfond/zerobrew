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

    #[arg(long, default_value = "48")]
    pub concurrency: usize,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Install {
        #[arg(required = true, num_args = 1..)]
        formulas: Vec<String>,
        #[arg(long)]
        no_link: bool,
    },
    Bundle {
        #[arg(long, short = 'f', value_name = "FILE", default_value = "Brewfile")]
        file: PathBuf,
        #[arg(long)]
        no_link: bool,
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
    Init,
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
}
