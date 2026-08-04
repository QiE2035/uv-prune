use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Errors that can occur during pruning.
#[derive(Error, Debug)]
pub enum PruneError {
    /// The uv cache directory does not exist.
    #[error("Cache directory does not exist: {0}")]
    CacheDirNotFound(PathBuf),

    /// The path is not a directory.
    #[error("Not a directory: {0}")]
    InvalidCacheDir(PathBuf),

    /// Failed to read a directory.
    #[error("Failed to read directory `{path}`: {source}")]
    ReadDirFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// Hard link check failed.
    #[error("Failed to check hard link status for `{path}`: {source}")]
    HardLinkCheckFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}
