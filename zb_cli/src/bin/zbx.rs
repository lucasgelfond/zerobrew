use console::style;
use std::env;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("zbx - Run a command from a formula without linking it");
        eprintln!();
        eprintln!("Usage: zbx <formula> [args...]");
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  zbx jq --version");
        eprintln!("  zbx wget https://example.com");
        std::process::exit(1);
    }

    let zbx_path = env::current_exe().expect("failed to get current executable path");
    let zbx_dir = zbx_path
        .parent()
        .expect("failed to get parent directory of zbx");
    let zb_path = zbx_dir.join("zb");

    let mut cmd = Command::new(&zb_path);
    cmd.arg("run").args(&args);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        eprintln!("{} {}", style("error:").red().bold(), err);
        std::process::exit(1);
    }

    #[cfg(not(unix))]
    {
        match cmd.status() {
            Ok(status) => {
                std::process::exit(status.code().unwrap_or(1));
            }
            Err(e) => {
                eprintln!("{} {}", style("error:").red().bold(), e);
                std::process::exit(1);
            }
        }
    }
}
