use std::path::PathBuf;

use crate::cli::Cli;

/// The uv cache bucket version this tool targets.
const ARCHIVE_VERSION: &str = "archive-v0";

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
        // `--cache-dir` and `UV_CACHE_DIR` are handled by clap.
        let cache_dir = cli.cache_dir.unwrap_or_else(default_cache_dir);

        let archive_dir = cache_dir.join(ARCHIVE_VERSION);

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

/// Fallback default cache directory based on platform.
fn default_cache_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("LOCALAPPDATA")
            .map(|dir| PathBuf::from(dir).join(r"uv\cache"))
            .unwrap_or_else(|_| {
                home::home_dir()
                    .map(|home| home.join(r"AppData\Local\uv\cache"))
                    .unwrap_or_else(|| std::env::temp_dir().join("uv-cache"))
            })
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
            PathBuf::from(xdg).join("uv")
        } else if let Some(home) = home::home_dir() {
            home.join(".cache").join("uv")
        } else {
            // No home directory — fall back to a stable temp location instead
            // of a literal `~` path, which would never expand.
            std::env::temp_dir().join("uv-cache")
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        compile_error!("Unsupported target OS");
    }
}
