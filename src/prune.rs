use std::fmt;
use std::fs;
use std::path::Path;

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
#[derive(Debug)]
enum ArchiveAction {
    /// Entry has `.dist-info` and is not hardlinked — should remove.
    Remove { name: String },
    /// Entry has no `.dist-info` — action depends on config.
    NoDistInfo,
    /// Entry is hardlinked — no action needed.
    Keep { name: String },
}

/// The action taken for an entry, shown in the report.
enum EntryAction {
    Deleting,
    Keeping,
    Skipping,
    Failed,
}

impl fmt::Display for EntryAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            EntryAction::Deleting => "Deleting",
            EntryAction::Keeping => "Keeping",
            EntryAction::Skipping => "Skipping",
            EntryAction::Failed => "Failed",
        };
        f.write_str(label)
    }
}

/// A per-entry outcome, deferred for sorted, aligned reporting.
struct EntryReport {
    level: log::Level,
    action: EntryAction,
    /// Primary identifier: package name if known, otherwise the archive id.
    entry: String,
    /// Supplementary detail, e.g. the archive id or a reason.
    detail: String,
    result: PruneResult,
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

    let entries = fs::read_dir(archive_dir).map_err(|e| PruneError::ReadDirFailed {
        path: archive_dir.clone(),
        source: e,
    })?;

    let reports: Vec<EntryReport> = entries
        .par_bridge()
        .map(|entry| match entry {
            Ok(e) => process_entry(e, config),
            Err(e) => {
                log::error!("Failed to read entry in {}: {e}", archive_dir.display());
                EntryReport {
                    level: log::Level::Error,
                    action: EntryAction::Failed,
                    entry: "(unknown)".to_string(),
                    detail: format!("failed to read entry: {e}"),
                    result: PruneResult {
                        errors: 1,
                        ..Default::default()
                    },
                }
            }
        })
        .collect();

    // Aggregate statistics.
    let mut total = PruneResult::default();
    for r in &reports {
        total.checked += r.result.checked;
        total.removed += r.result.removed;
        total.skipped += r.result.skipped;
        total.no_dist_info += r.result.no_dist_info;
        total.errors += r.result.errors;
    }

    // Report entries sorted by name (archive id when the name is unknown).
    let mut reports = reports;
    reports.sort_by(|a, b| a.entry.cmp(&b.entry));

    // Column width adapts to the longest entry name.
    let entry_width = reports
        .iter()
        .map(|r| r.entry.chars().count())
        .max()
        .unwrap_or(0);

    let dry_run_prefix = if config.dry_run { "[DRY-RUN] " } else { "" };
    for r in &reports {
        log::log!(
            r.level,
            "{dry_run_prefix}{}",
            format_entry(&r.action, &r.entry, &r.detail, entry_width)
        );
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
fn process_entry(entry: fs::DirEntry, config: &Config) -> EntryReport {
    let path = entry.path();
    let archive_id = entry.file_name().to_string_lossy().to_string();

    if !path.is_dir() {
        return EntryReport {
            level: log::Level::Debug,
            action: EntryAction::Skipping,
            entry: archive_id,
            detail: "not a directory".to_string(),
            result: PruneResult {
                checked: 1,
                skipped: 1,
                ..Default::default()
            },
        };
    }

    match classify_archive(&path) {
        Ok(ArchiveAction::Remove { name }) => {
            let result = PruneResult {
                checked: 1,
                removed: 1,
                ..Default::default()
            };
            if config.dry_run {
                return EntryReport {
                    level: log::Level::Info,
                    action: EntryAction::Deleting,
                    entry: name,
                    detail: archive_id,
                    result,
                };
            }
            match fs::remove_dir_all(&path) {
                Ok(()) => EntryReport {
                    level: log::Level::Info,
                    action: EntryAction::Deleting,
                    entry: name,
                    detail: archive_id,
                    result,
                },
                Err(e) => EntryReport {
                    level: log::Level::Error,
                    action: EntryAction::Failed,
                    entry: name,
                    detail: format!("{archive_id}: {e}"),
                    result: PruneResult {
                        checked: 1,
                        errors: 1,
                        ..Default::default()
                    },
                },
            }
        }
        Ok(ArchiveAction::NoDistInfo) => {
            if config.include_no_dist_info {
                let result = PruneResult {
                    checked: 1,
                    removed: 1,
                    no_dist_info: 1,
                    ..Default::default()
                };
                if config.dry_run {
                    return EntryReport {
                        level: log::Level::Info,
                        action: EntryAction::Deleting,
                        entry: archive_id,
                        detail: "no .dist-info".to_string(),
                        result,
                    };
                }
                match fs::remove_dir_all(&path) {
                    Ok(()) => EntryReport {
                        level: log::Level::Info,
                        action: EntryAction::Deleting,
                        entry: archive_id,
                        detail: "no .dist-info".to_string(),
                        result,
                    },
                    Err(e) => EntryReport {
                        level: log::Level::Error,
                        action: EntryAction::Failed,
                        entry: archive_id,
                        detail: format!("no .dist-info: {e}"),
                        result: PruneResult {
                            checked: 1,
                            errors: 1,
                            ..Default::default()
                        },
                    },
                }
            } else {
                EntryReport {
                    level: log::Level::Warn,
                    action: EntryAction::Skipping,
                    entry: archive_id,
                    detail: "no .dist-info (use --include-no-dist-info to remove)".to_string(),
                    result: PruneResult {
                        checked: 1,
                        skipped: 1,
                        no_dist_info: 1,
                        ..Default::default()
                    },
                }
            }
        }
        Ok(ArchiveAction::Keep { name }) => EntryReport {
            level: log::Level::Debug,
            action: EntryAction::Keeping,
            entry: name,
            detail: archive_id,
            result: PruneResult {
                checked: 1,
                skipped: 1,
                ..Default::default()
            },
        },
        Err(e) => EntryReport {
            level: log::Level::Error,
            action: EntryAction::Failed,
            entry: archive_id,
            detail: e.to_string(),
            result: PruneResult {
                checked: 1,
                errors: 1,
                ..Default::default()
            },
        },
    }
}

/// Format a per-entry line as aligned columns: `ACTION  ENTRY  DETAIL`.
///
/// The `entry` column is padded to `entry_width` characters so that the
/// `detail` column starts at the same position on every line.
fn format_entry(
    action: impl fmt::Display,
    entry: &str,
    detail: &str,
    entry_width: usize,
) -> String {
    format!("{action:<10} {entry:<width$} {detail}", width = entry_width)
}

/// Determine what action to take for an archive entry.
fn classify_archive(archive_path: &Path) -> Result<ArchiveAction, PruneError> {
    // Look for a .dist-info subdirectory
    let dist_info = fs::read_dir(archive_path)
        .map_err(|e| PruneError::ReadDirFailed {
            path: archive_path.to_path_buf(),
            source: e,
        })?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| PruneError::ReadDirFailed {
            path: archive_path.to_path_buf(),
            source: e,
        })?
        .into_iter()
        .find(|p| p.to_string_lossy().ends_with(DIST_INFO));

    let dist_info = match dist_info {
        Some(p) => p,
        None => return Ok(ArchiveAction::NoDistInfo),
    };

    // The package name and version, e.g. `anyio-4.0.0` from `anyio-4.0.0.dist-info`.
    let name = dist_info
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .strip_suffix(DIST_INFO)
        .unwrap_or_default()
        .to_string();

    // Check hard link status via METADATA file
    let metadata_path = dist_info.join(HARD_LINK_CHECK_FILE);
    match metadata_path.is_hardlink() {
        Ok(true) => Ok(ArchiveAction::Keep { name }),
        Ok(false) => Ok(ArchiveAction::Remove { name }),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// Create a unique temporary directory for a test.
    fn temp_dir(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("uv-prune-test-{label}-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Create an archive entry with a `.dist-info` directory and `METADATA`.
    fn write_archive(root: &Path, id: &str) -> PathBuf {
        let archive = root.join(id);
        let dist_info = archive.join(format!("{id}.dist-info"));
        fs::create_dir_all(&dist_info).unwrap();
        fs::write(
            dist_info.join(HARD_LINK_CHECK_FILE),
            "Metadata-Version: 2.1\nName: test\nVersion: 0.0.0\n",
        )
        .unwrap();
        archive
    }

    fn test_config(dry_run: bool) -> Config {
        Config {
            cache_dir: PathBuf::new(),
            archive_dir: PathBuf::new(),
            dry_run,
            verbose: false,
            include_no_dist_info: false,
            jobs: 0,
            timing: false,
        }
    }

    #[test]
    fn classifies_archive_with_dist_info_as_removable() {
        let root = temp_dir("classify-remove");
        let archive = write_archive(&root, "anyio-4.0.0");
        match classify_archive(&archive).unwrap() {
            ArchiveAction::Remove { name } => assert_eq!(name, "anyio-4.0.0"),
            other => panic!("expected Remove, got {other:?}"),
        }
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn classifies_archive_without_dist_info() {
        let root = temp_dir("classify-nodist");
        let archive = root.join("mystery-1.0");
        fs::create_dir_all(&archive).unwrap();
        assert!(matches!(
            classify_archive(&archive).unwrap(),
            ArchiveAction::NoDistInfo
        ));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn classifies_hardlinked_archive_as_keep() {
        let root = temp_dir("classify-keep");
        let archive = write_archive(&root, "anyio-4.0.0");
        let metadata = archive.join("anyio-4.0.0.dist-info").join(HARD_LINK_CHECK_FILE);
        fs::hard_link(&metadata, &metadata.with_file_name("METADATA-link")).unwrap();
        match classify_archive(&archive).unwrap() {
            ArchiveAction::Keep { name } => assert_eq!(name, "anyio-4.0.0"),
            other => panic!("expected Keep, got {other:?}"),
        }
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn classify_missing_archive_errors() {
        let root = temp_dir("classify-missing");
        let archive = root.join("nope-1.0");
        assert!(classify_archive(&archive).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn dry_run_reports_deleting_without_removing() {
        let root = temp_dir("dry-run");
        let archive = write_archive(&root, "requests-2.31.0");
        let config = test_config(true);
        let entry = fs::read_dir(&root).unwrap().next().unwrap().unwrap();

        let report = process_entry(entry, &config);

        assert!(matches!(report.action, EntryAction::Deleting));
        assert_eq!(report.result.removed, 1);
        assert!(archive.exists(), "dry-run must not delete anything");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn process_entry_removes_archive() {
        let root = temp_dir("remove");
        let archive = write_archive(&root, "rich-13.7.0");
        let config = test_config(false);
        let entry = fs::read_dir(&root).unwrap().next().unwrap().unwrap();

        let report = process_entry(entry, &config);

        assert!(matches!(report.action, EntryAction::Deleting));
        assert_eq!(report.result.removed, 1);
        assert!(!archive.exists(), "entry should have been removed");
        fs::remove_dir_all(&root).unwrap();
    }
}
