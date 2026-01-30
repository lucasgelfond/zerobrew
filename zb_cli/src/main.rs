use clap::Parser;
use console::style;
use zb_io::install::create_installer;

mod cli;
mod commands;
mod init;
mod utils;

use cli::{Cli, Commands};
use init::ensure_init;
use utils::get_root_path;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        eprintln!("{} {}", style("error:").red().bold(), e);
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), zb_core::Error> {
    if let Commands::Completion { shell } = cli.command {
        return commands::completion::execute(shell);
    }

    let root = get_root_path(cli.root);
    let prefix = cli.prefix.unwrap_or_else(|| root.join("prefix"));

    if matches!(cli.command, Commands::Init) {
        return commands::init::execute(&root, &prefix);
    }

    if !matches!(cli.command, Commands::Reset { .. }) {
        ensure_init(&root, &prefix)?;
    }

    let mut installer = create_installer(&root, &prefix, cli.concurrency)?;

    match cli.command {
        Commands::Init => unreachable!(),
        Commands::Completion { .. } => unreachable!(),
        Commands::Install { formula, no_link } => {
            commands::install::execute(&mut installer, formula, no_link).await
        }
        Commands::Uninstall { formula } => commands::uninstall::execute(&mut installer, formula),
        Commands::Migrate { yes, force } => {
            commands::migrate::execute(&mut installer, yes, force).await
        }
        Commands::List => commands::list::execute(&mut installer),
        Commands::Info { formula } => commands::info::execute(&mut installer, formula),
        Commands::Gc => commands::gc::execute(&mut installer),
        Commands::Reset { yes } => commands::reset::execute(&root, &prefix, yes),
    }
}
