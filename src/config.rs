use std::path::PathBuf;

use crate::cli::Cli;

/// Aggregated configuration from CLI args and environment defaults.
#[derive(Debug)]
pub struct Config {
    /// The base uv cache directory.
    pub cache_dir: PathBuf,
    /// The archive-v0 directory to prune.
    pub archive_dir: PathBuf,
    /// If true, only print actions without deleting.
    pub dry_run: bool,
    /// Enable verbose (debug) logging.
    pub verbose: bool,
    /// If true, also remove entries without .dist-info.
    pub include_no_dist_info: bool,
    /// Number of parallel workers (0 = auto-detect).
    pub jobs: usize,
    /// If true, measure and print elapsed time.
    pub timing: bool,
}

impl From<Cli> for Config {
    fn from(cli: Cli) -> Self {
        let cache_dir = cli
            .cache_dir
            .or_else(detect_cache_dir_from_env)
            .unwrap_or_else(default_cache_dir);

        let archive_dir = cache_dir.join("archive-v0");

        Config {
            timing: !cli.no_timing,
            dry_run: cli.dry_run,
            verbose: cli.verbose,
            include_no_dist_info: cli.include_no_dist_info,
            jobs: cli.jobs,
            cache_dir,
            archive_dir,
        }
    }
}

/// Detect cache directory from environment variable.
fn detect_cache_dir_from_env() -> Option<PathBuf> {
    std::env::var("UV_CACHE_DIR").ok().map(PathBuf::from)
}

/// Fallback default cache directory based on platform.
fn default_cache_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let local_app_data =
            std::env::var("LOCALAPPDATA").expect("LOCALAPPDATA must be set on Windows");
        PathBuf::from(local_app_data).join(r"uv\cache")
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
            PathBuf::from(xdg).join("uv")
        } else if let Some(home) = home::home_dir() {
            home.join(".cache").join("uv")
        } else {
            PathBuf::from("~/.cache/uv")
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        compile_error!("Unsupported target OS");
    }
}
