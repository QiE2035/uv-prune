use std::time::Instant;

use clap::Parser;

use crate::cli::Cli;
use crate::config::Config;

mod cli;
mod config;
mod error;
mod hardlink;
mod prune;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::from(cli);

    // Initialize logger — verbose maps to debug level
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(if config.verbose { "debug" } else { "info" }),
    )
    .format_timestamp(None)
    .init();

    log::info!(
        "uv-prune v{} — cache: {}",
        env!("CARGO_PKG_VERSION"),
        config.cache_dir.display()
    );
    log::debug!("Config: {config:#?}");

    // Configure Rayon thread pool if jobs is specified
    if config.jobs > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(config.jobs)
            .build_global()
            .ok();
    }

    // Optional validation
    prune::validate_cache_dir(&config)?;

    // Run prune (optionally timed)
    let result = if config.timing {
        let start = Instant::now();
        let result = prune::run(&config)?;
        let elapsed = start.elapsed();
        log::info!("Elapsed time: {elapsed:?}");
        result
    } else {
        prune::run(&config)?
    };

    // Non-zero exit if there were errors
    if result.errors > 0 {
        eprintln!("Completed with {} error(s)", result.errors);
        std::process::exit(1);
    }

    Ok(())
}
