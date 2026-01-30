use clap::{CommandFactory, Parser};
use clap_complete::generate;
use std::io;

#[derive(Parser)]
#[command(name = "zb")]
#[command(about = "Zerobrew - A fast Homebrew-compatible package installer")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    command: crate::cli::Commands,
}

pub fn execute(shell: clap_complete::shells::Shell) -> Result<(), zb_core::Error> {
    let mut cmd = crate::cli::Cli::command();
    generate(shell, &mut cmd, "zb", &mut io::stdout());
    Ok(())
}
