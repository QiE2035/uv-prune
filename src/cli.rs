use std::path::PathBuf;

use clap::Parser;
use clap_complete::Shell;

/// Clean uv cache by removing non-hardlinked archive entries.
#[derive(Parser, Debug)]
#[command(version = env!("UV_PRUNE_FULL_VERSION"), author)]
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
    #[arg(short, long, default_value = "0")]
    pub jobs: usize,

    /// Disable timing measurement
    #[arg(short, long)]
    pub no_timing: bool,

    /// Generate a shell completion script and exit
    #[arg(short, long, value_name = "SHELL", hide = false)]
    pub generate_completions: Option<Shell>,
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser, ValueEnum};

    use super::*;

    #[test]
    fn generate_completions_flag_parses_shell() {
        let cli = Cli::try_parse_from([env!("CARGO_PKG_NAME"), "--generate-completions", "bash"])
            .unwrap();
        assert!(matches!(cli.generate_completions, Some(Shell::Bash)));

        // The flag stays unset when it is not passed.
        let plain = Cli::try_parse_from([env!("CARGO_PKG_NAME")]).unwrap();
        assert!(plain.generate_completions.is_none());
    }

    #[test]
    fn generates_nonempty_completion_for_every_shell() {
        for shell in Shell::value_variants() {
            let mut cmd = Cli::command();
            let mut out: Vec<u8> = Vec::new();
            let bin_name = cmd.get_name().to_owned();
            clap_complete::generate(*shell, &mut cmd, &bin_name, &mut out);
            assert!(!out.is_empty(), "completion for {shell} is empty");
        }
    }
}
