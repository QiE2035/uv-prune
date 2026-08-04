use std::path::PathBuf;

use clap::Parser;

/// Clean uv cache by removing non-hardlinked archive entries.
#[derive(Parser, Debug)]
#[command(name = "uv-prune", version, author)]
pub struct Cli {
    /// UV cache directory (overrides UV_CACHE_DIR env var)
    #[arg(short, long, env = "UV_CACHE_DIR")]
    pub cache_dir: Option<PathBuf>,

    /// Show what would be deleted without actually deleting
    #[arg(short, long)]
    pub dry_run: bool,

    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Also remove entries without a .dist-info directory
    #[arg(short, long)]
    pub include_no_dist_info: bool,

    /// Number of parallel workers (0 = auto)
    #[arg(short, long)]
    pub jobs: usize,

    /// Disable timing measurement
    #[arg(short, long)]
    pub no_timing: bool,
}
