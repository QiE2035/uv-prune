use std::fs;
use std::path::PathBuf;

use rayon::iter::{ParallelBridge, ParallelIterator};

use crate::config::Config;
use crate::error::PruneError;
use crate::hardlink::IsHardLink;

/// File name to check for hard link status inside `.dist-info`.
const HARD_LINK_CHECK_FILE: &str = "METADATA";
/// The dist-info directory suffix.
const DIST_INFO: &str = ".dist-info";

/// Statistics from a prune run.
#[derive(Debug, Default)]
pub struct PruneResult {
    /// Total entries checked.
    pub checked: usize,
    /// Entries removed.
    pub removed: usize,
    /// Entries skipped (hardlinked, still in use).
    pub skipped: usize,
    /// Entries with no `.dist-info` (warned or removed based on config).
    pub no_dist_info: usize,
    /// Number of errors encountered.
    pub errors: usize,
}

/// Reasons an archive entry may be removed.
enum ArchiveAction {
    /// Entry has `.dist-info` and is not hardlinked — should remove.
    Remove { name: String },
    /// Entry has no `.dist-info` — action depends on config.
    NoDistInfo,
    /// Entry is hardlinked — no action needed.
    Keep,
}

/// Run the prune operation with the given configuration.
pub fn run(config: &Config) -> anyhow::Result<PruneResult> {
    let archive_dir = &config.archive_dir;

    if !archive_dir.exists() {
        anyhow::bail!(PruneError::CacheDirNotFound(archive_dir.clone()));
    }
    if !archive_dir.is_dir() {
        anyhow::bail!(PruneError::InvalidCacheDir(archive_dir.clone()));
    }

    log::info!("Pruning archive directory: {}", archive_dir.display());
    if config.dry_run {
        log::info!("Dry-run mode — no files will be deleted");
    }

    let entries = fs::read_dir(archive_dir)
        .map_err(|e| PruneError::ReadDirFailed {
            path: archive_dir.clone(),
            source: e,
        })?;

    let results: Vec<PruneResult> = entries
        .par_bridge()
        .filter_map(|entry| {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    log::error!("Failed to read entry in {}: {e}", archive_dir.display());
                    return Some(PruneResult {
                        errors: 1,
                        ..Default::default()
                    });
                }
            };
            Some(process_entry(entry, config))
        })
        .collect();

    let mut total = PruneResult::default();
    for r in results {
        total.checked += r.checked;
        total.removed += r.removed;
        total.skipped += r.skipped;
        total.no_dist_info += r.no_dist_info;
        total.errors += r.errors;
    }

    // Print summary
    log::info!(
        "Done: {} checked, {} removed, {} skipped, {} no-dist-info, {} errors",
        total.checked,
        total.removed,
        total.skipped,
        total.no_dist_info,
        total.errors,
    );

    Ok(total)
}

/// Process a single archive entry.
fn process_entry(entry: fs::DirEntry, config: &Config) -> PruneResult {
    let path = entry.path();
    let entry_id = entry.file_name().to_string_lossy().to_string();

    if !path.is_dir() {
        log::debug!("Skipping non-directory entry: {entry_id}");
        return PruneResult {
            checked: 1,
            skipped: 1,
            ..Default::default()
        };
    }

    match classify_archive(&path) {
        Ok(ArchiveAction::Remove { name }) => {
            let label = format!("{entry_id} ({name})");
            if config.dry_run {
                log::info!("[DRY-RUN] Would delete: {label}");
            } else {
                log::info!("Deleting: {label}");
                if let Err(e) = fs::remove_dir_all(&path) {
                    log::error!("Failed to delete {label}: {e}");
                    return PruneResult {
                        checked: 1,
                        errors: 1,
                        ..Default::default()
                    };
                }
            }
            PruneResult {
                checked: 1,
                removed: 1,
                ..Default::default()
            }
        }
        Ok(ArchiveAction::NoDistInfo) => {
            if config.include_no_dist_info {
                if config.dry_run {
                    log::info!("[DRY-RUN] Would delete (no .dist-info): {entry_id}");
                } else {
                    log::info!("Deleting (no .dist-info): {entry_id}");
                    if let Err(e) = fs::remove_dir_all(&path) {
                        log::error!("Failed to delete {entry_id}: {e}");
                        return PruneResult {
                            checked: 1,
                            errors: 1,
                            ..Default::default()
                        };
                    }
                }
                PruneResult {
                    checked: 1,
                    removed: 1,
                    no_dist_info: 1,
                    ..Default::default()
                }
            } else {
                log::warn!(
                    "Skipping {entry_id}: no .dist-info found (use --include-no-dist-info to remove)"
                );
                PruneResult {
                    checked: 1,
                    skipped: 1,
                    no_dist_info: 1,
                    ..Default::default()
                }
            }
        }
        Ok(ArchiveAction::Keep) => {
            log::debug!("Keeping (hardlinked): {entry_id}");
            PruneResult {
                checked: 1,
                skipped: 1,
                ..Default::default()
            }
        }
        Err(e) => {
            log::error!("Error processing {entry_id}: {e}");
            PruneResult {
                checked: 1,
                errors: 1,
                ..Default::default()
            }
        }
    }
}

/// Determine what action to take for an archive entry.
fn classify_archive(archive_path: &PathBuf) -> Result<ArchiveAction, PruneError> {
    // Look for a .dist-info subdirectory
    let dist_info = fs::read_dir(archive_path)
        .map_err(|e| PruneError::ReadDirFailed {
            path: archive_path.clone(),
            source: e,
        })?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.to_string_lossy().ends_with(DIST_INFO));

    let dist_info = match dist_info {
        Some(p) => p,
        None => return Ok(ArchiveAction::NoDistInfo),
    };

    // Check hard link status via METADATA file
    let metadata_path = dist_info.join(HARD_LINK_CHECK_FILE);
    match metadata_path.is_hardlink() {
        Ok(true) => Ok(ArchiveAction::Keep),
        Ok(false) => {
            let name = dist_info
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .trim_end_matches(DIST_INFO)
                .to_string();
            Ok(ArchiveAction::Remove { name })
        }
        Err(e) => Err(PruneError::HardLinkCheckFailed {
            path: metadata_path,
            source: e,
        }),
    }
}

/// Check whether the archive-v0 directory is accessible and looks valid.
pub fn validate_cache_dir(config: &Config) -> anyhow::Result<()> {
    let archive_dir = &config.archive_dir;

    if !archive_dir.exists() {
        log::warn!(
            "Archive directory does not exist: {}",
            archive_dir.display()
        );
        return Ok(());
    }

    if !archive_dir.is_dir() {
        anyhow::bail!(PruneError::InvalidCacheDir(archive_dir.clone()));
    }

    log::debug!("Archive directory exists: {}", archive_dir.display());
    Ok(())
}
