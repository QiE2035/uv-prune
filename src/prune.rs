use std::fs;
use std::path::Path;

use rayon::iter::{ParallelBridge, ParallelIterator};

use crate::config::Config;
use crate::error::PruneError;
use crate::hardlink::IsHardLink;
use crate::package::Package;
use crate::report::{EntryReport, PruneResult, report_entries};

/// File name to check for hard link status inside `.dist-info`.
const HARD_LINK_CHECK_FILE: &str = "METADATA";
/// The dist-info directory suffix.
const DIST_INFO: &str = ".dist-info";

/// Reasons an archive entry may be removed.
#[derive(Debug)]
enum ArchiveAction {
    /// Entry has `.dist-info` and is not hardlinked — should remove.
    Remove(Package),
    /// Entry has no `.dist-info` — action depends on config.
    NoDistInfo,
    /// Entry is hardlinked — no action needed.
    Keep(Package),
}

/// Map an I/O error from reading a directory to a [`PruneError::ReadDirFailed`].
fn read_dir_failed(path: &Path) -> impl Fn(std::io::Error) -> PruneError {
    let path = path.to_path_buf();
    move |source| PruneError::ReadDirFailed {
        path: path.clone(),
        source,
    }
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

    let entries = fs::read_dir(archive_dir).map_err(read_dir_failed(archive_dir))?;

    let reports: Vec<EntryReport> = entries
        .par_bridge()
        .map(|entry| match entry {
            Ok(e) => process_entry(e, config),
            Err(e) => {
                log::error!("Failed to read entry in {}: {e}", archive_dir.display());
                EntryReport::unreadable(format!("failed to read entry: {e}"))
            }
        })
        .collect();

    let total = report_entries(reports, config.dry_run);
    if total.no_dist_info > 0 && !config.include_no_dist_info {
        let noun = if total.no_dist_info == 1 {
            "entry"
        } else {
            "entries"
        };
        log::info!(
            "{total_no_dist_info} {noun} had no .dist-info directory — re-run with --include-no-dist-info to remove them",
            total_no_dist_info = total.no_dist_info,
        );
    }
    Ok(total)
}

/// Process a single archive entry.
fn process_entry(entry: fs::DirEntry, config: &Config) -> EntryReport {
    let path = entry.path();
    let archive_id = entry.file_name().to_string_lossy().to_string();

    if !path.is_dir() {
        return EntryReport::skipped(archive_id, "not a directory", false);
    }

    match classify_archive(&path) {
        Ok(ArchiveAction::Remove(pkg)) => {
            remove_entry(archive_id, Some(pkg), None, &path, config, false)
        }
        Ok(ArchiveAction::NoDistInfo) => {
            if config.include_no_dist_info {
                remove_entry(
                    archive_id,
                    None,
                    Some("no .dist-info".to_string()),
                    &path,
                    config,
                    true,
                )
            } else {
                EntryReport::skipped(
                    archive_id,
                    "no .dist-info (use --include-no-dist-info to remove)",
                    true,
                )
            }
        }
        Ok(ArchiveAction::Keep(pkg)) => EntryReport::kept(archive_id, pkg),
        Err(e) => EntryReport::failed(archive_id, None, e.to_string()),
    }
}

/// Remove an archive entry (or report it in dry-run mode).
///
/// `detail` explains what was being removed when the deletion fails,
/// e.g. `"no .dist-info"`; for regular removals pass an empty string.
fn remove_entry(
    archive_id: String,
    pkg: Option<Package>,
    detail: Option<String>,
    path: &Path,
    config: &Config,
    no_dist_info: bool,
) -> EntryReport {
    if config.dry_run {
        let detail = detail.filter(|d| !d.is_empty());
        return EntryReport::deleting(archive_id, pkg, detail, no_dist_info);
    }
    match fs::remove_dir_all(path) {
        Ok(()) => EntryReport::deleting(
            archive_id,
            pkg,
            detail.filter(|d| !d.is_empty()),
            no_dist_info,
        ),
        Err(e) => {
            let detail = match detail {
                Some(d) if !d.is_empty() => format!("{d}: {e}"),
                _ => e.to_string(),
            };
            EntryReport::failed(archive_id, pkg, detail)
        }
    }
}

/// Determine what action to take for an archive entry.
fn classify_archive(archive_path: &Path) -> Result<ArchiveAction, PruneError> {
    // Look for a .dist-info subdirectory
    let read_dir_err = read_dir_failed(archive_path);
    let dist_info = fs::read_dir(archive_path)
        .map_err(&read_dir_err)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(read_dir_err)?
        .into_iter()
        .find(|p| p.to_string_lossy().ends_with(DIST_INFO));

    let dist_info = match dist_info {
        Some(p) => p,
        None => return Ok(ArchiveAction::NoDistInfo),
    };

    // The package name and version, e.g. `anyio` and `4.0.0` from `anyio-4.0.0.dist-info`.
    let dist_info_name = dist_info
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .strip_suffix(DIST_INFO)
        .unwrap_or_default()
        .to_string();
    let pkg = Package::from(dist_info_name.as_str());

    // Check hard link status via METADATA file
    let metadata_path = dist_info.join(HARD_LINK_CHECK_FILE);
    match metadata_path.is_hardlink() {
        Ok(true) => Ok(ArchiveAction::Keep(pkg)),
        Ok(false) => Ok(ArchiveAction::Remove(pkg)),
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
    use crate::report::EntryAction;
    use std::fs;
    use std::path::PathBuf;

    /// Create a unique temporary directory for a test.
    fn temp_dir(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("{}-test-{label}-{unique}", env!("CARGO_PKG_NAME")));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Create an archive entry with a `.dist-info` directory and `METADATA`.
    fn write_archive(root: &Path, name_version: &str) -> PathBuf {
        let archive = root.join(name_version);
        let dist_info = archive.join(format!("{name_version}.dist-info"));
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
            ArchiveAction::Remove(pkg) => {
                assert_eq!(pkg.name, "anyio");
                assert_eq!(pkg.version.as_deref(), Some("4.0.0"));
            }
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
        let metadata = archive
            .join("anyio-4.0.0.dist-info")
            .join(HARD_LINK_CHECK_FILE);
        fs::hard_link(&metadata, metadata.with_file_name("METADATA-link")).unwrap();
        match classify_archive(&archive).unwrap() {
            ArchiveAction::Keep(pkg) => {
                assert_eq!(pkg.name, "anyio");
                assert_eq!(pkg.version.as_deref(), Some("4.0.0"));
            }
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
