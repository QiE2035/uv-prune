use std::time::Instant;

use clap::Parser;

use crate::cli::Cli;
use crate::config::Config;
use crate::report::PruneResult;

mod cli;
mod config;
mod error;
mod hardlink;
mod package;
mod prune;
mod report;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::from(cli);

    init_logging(&config);
    init_thread_pool(config.jobs);

    log::info!(
        "uv-prune v{} — cache: {}",
        env!("CARGO_PKG_VERSION"),
        config.cache_dir.display()
    );
    log::debug!("Config: {config:#?}");

    // Optional validation
    prune::validate_cache_dir(&config)?;

    // Run prune (optionally timed)
    let result = run_prune(&config)?;

    // Non-zero exit if there were errors
    if result.errors > 0 {
        log::error!("Completed with {} error(s)", result.errors);
        std::process::exit(1);
    }

    Ok(())
}

/// Initialize the logger — verbose maps to debug level.
fn init_logging(config: &Config) {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(if config.verbose { "debug" } else { "info" }),
    )
    .format_timestamp(None)
    .init();
}

/// Configure the Rayon global thread pool if a job count is specified.
fn init_thread_pool(jobs: usize) {
    if jobs > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build_global()
            .ok();
    }
}

/// Run the prune operation, measuring elapsed time when enabled.
fn run_prune(config: &Config) -> anyhow::Result<PruneResult> {
    if !config.timing {
        return prune::run(config);
    }
    let start = Instant::now();
    let result = prune::run(config)?;
    log::info!("Elapsed time: {:?}", start.elapsed());
    Ok(result)
}
