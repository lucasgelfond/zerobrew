use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub fn init(verbose: u8, quiet: bool) {
    let level = if quiet {
        LevelFilter::ERROR
    } else {
        match verbose {
            0 => LevelFilter::WARN,
            1 => LevelFilter::INFO,
            2 => LevelFilter::DEBUG,
            _ => LevelFilter::TRACE,
        }
    };

    let filter = EnvFilter::builder()
        .with_default_directive(level.into())
        .from_env_lossy();

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .without_time(),
        )
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::init;

    #[test]
    fn init_is_idempotent() {
        init(0, false);
        init(2, false);
        init(0, true);
    }
}
