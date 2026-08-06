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

impl std::ops::AddAssign<&PruneResult> for PruneResult {
    fn add_assign(&mut self, rhs: &PruneResult) {
        self.checked += rhs.checked;
        self.removed += rhs.removed;
        self.skipped += rhs.skipped;
        self.no_dist_info += rhs.no_dist_info;
        self.errors += rhs.errors;
    }
}

impl<'a> std::iter::Sum<&'a PruneResult> for PruneResult {
    fn sum<I: Iterator<Item = &'a PruneResult>>(iter: I) -> Self {
        iter.fold(PruneResult::default(), |mut acc, r| {
            acc += r;
            acc
        })
    }
}

impl PruneResult {
    /// One entry counted as checked and removed (optionally a no-dist-info entry).
    fn removed(no_dist_info: bool) -> Self {
        PruneResult {
            checked: 1,
            removed: 1,
            no_dist_info: usize::from(no_dist_info),
            ..Default::default()
        }
    }

    /// One entry counted as checked and skipped (optionally a no-dist-info entry).
    fn skipped(no_dist_info: bool) -> Self {
        PruneResult {
            checked: 1,
            skipped: 1,
            no_dist_info: usize::from(no_dist_info),
            ..Default::default()
        }
    }

    /// One checked entry that failed to process.
    fn failed() -> Self {
        PruneResult {
            checked: 1,
            errors: 1,
            ..Default::default()
        }
    }

    /// One unreadable entry (counted as an error, not as checked).
    fn unreadable() -> Self {
        PruneResult {
            errors: 1,
            ..Default::default()
        }
    }
}

/// A package identity derived from its `.dist-info` directory name.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Package {
    /// Normalized package name.
    name: String,
    /// Package version, if known.
    version: Option<String>,
}

/// Parse a dist-info directory name into a package identity.
///
/// uv names these `{name}-{version}.dist-info`, where the name is normalized
/// (PEP 503: `-` is replaced with `_`), so the first `-` separates the two.
impl From<&str> for Package {
    fn from(dist_info_name: &str) -> Self {
        match dist_info_name.split_once('-') {
            Some((name, version)) => Package {
                name: name.to_string(),
                version: Some(version.to_string()),
            },
            // No `-` at all — treat the whole name as the package name.
            None => Package {
                name: dist_info_name.to_string(),
                version: None,
            },
        }
    }
}

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

/// The action taken for an entry, shown in the report.
enum EntryAction {
    Deleting,
    Keeping,
    Skipping,
    Failed,
}

impl EntryAction {
    /// The action label shown in the report.
    fn label(&self) -> &'static str {
        match self {
            EntryAction::Deleting => "Deleting",
            EntryAction::Keeping => "Keeping",
            EntryAction::Skipping => "Skipping",
            EntryAction::Failed => "Failed",
        }
    }
}

impl fmt::Display for EntryAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// A per-entry outcome, deferred for sorted, aligned reporting.
struct EntryReport {
    level: log::Level,
    action: EntryAction,
    /// Archive id (the uv cache directory name), if known.
    id: Option<String>,
    /// Package identity, if a `.dist-info` directory was found.
    pkg: Option<Package>,
    /// Supplementary detail, e.g. a reason, if any.
    detail: Option<String>,
    result: PruneResult,
}

impl EntryReport {
    /// Entry kept because it is still hardlinked in another environment.
    fn kept(id: String, pkg: Package) -> Self {
        EntryReport {
            level: log::Level::Debug,
            action: EntryAction::Keeping,
            id: Some(id),
            pkg: Some(pkg),
            detail: None,
            result: PruneResult::skipped(false),
        }
    }

    /// Entry skipped because of a reason, e.g. not a directory or missing
    /// dist-info (the latter is reported at warn level).
    fn skipped(id: String, detail: impl Into<String>, no_dist_info: bool) -> Self {
        EntryReport {
            level: if no_dist_info {
                log::Level::Warn
            } else {
                log::Level::Debug
            },
            action: EntryAction::Skipping,
            id: Some(id),
            pkg: None,
            detail: Some(detail.into()),
            result: PruneResult::skipped(no_dist_info),
        }
    }

    /// Entry removed, or scheduled for removal in dry-run mode.
    fn deleting(
        id: String,
        pkg: Option<Package>,
        detail: Option<String>,
        no_dist_info: bool,
    ) -> Self {
        EntryReport {
            level: log::Level::Info,
            action: EntryAction::Deleting,
            id: Some(id),
            pkg,
            detail,
            result: PruneResult::removed(no_dist_info),
        }
    }

    /// Entry examined but failed to process.
    fn failed(id: String, pkg: Option<Package>, detail: String) -> Self {
        EntryReport {
            level: log::Level::Error,
            action: EntryAction::Failed,
            id: Some(id),
            pkg,
            detail: Some(detail),
            result: PruneResult::failed(),
        }
    }

    /// Directory entry that could not be read at all (never counted as checked).
    fn unreadable(detail: String) -> Self {
        EntryReport {
            level: log::Level::Error,
            action: EntryAction::Failed,
            id: None,
            pkg: None,
            detail: Some(detail),
            result: PruneResult::unreadable(),
        }
    }
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

    Ok(report_entries(reports, config.dry_run))
}

/// Sort, print and summarize the per-entry reports.
fn report_entries(mut reports: Vec<EntryReport>, dry_run: bool) -> PruneResult {
    // Aggregate statistics.
    let total: PruneResult = reports.iter().map(|r| &r.result).sum();

    // Report entries sorted by package (name, then version), then id.
    reports.sort_by(|a, b| a.pkg.cmp(&b.pkg).then_with(|| a.id.cmp(&b.id)));

    // Column widths adapt to the longest value in each column.
    let widths = ColumnWidths::from(reports.as_slice());

    let dry_run_prefix = if dry_run { "[DRY-RUN] " } else { "" };
    log::info!("{dry_run_prefix}{}", format_header(&widths));
    log::info!("{dry_run_prefix}{}", format_separator(&widths));
    for r in &reports {
        log::log!(r.level, "{dry_run_prefix}{}", format_entry(r, &widths));
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

    total
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

/// A column of the report table.
#[derive(Clone, Copy)]
struct Column {
    header: &'static str,
    width: usize,
}

/// Two-space gap between report table cells.
const SEPARATOR: &str = "  ";

/// Column widths for the report table, derived from the longest value in
/// each column so that every line is aligned without wasting space.
struct ColumnWidths([Column; ColumnWidths::COLUMNS.len()]);

impl ColumnWidths {
    /// The report table columns, in display order.
    const COLUMNS: [Column; 5] = [
        Column {
            header: "Action",
            width: 0,
        },
        Column {
            header: "ID",
            width: 0,
        },
        Column {
            header: "Version",
            width: 0,
        },
        Column {
            header: "Name",
            width: 0,
        },
        Column {
            header: "Detail",
            width: 0,
        },
    ];
}

const PLACEHOLDER: &str = "-";

impl From<&[EntryReport]> for ColumnWidths {
    fn from(reports: &[EntryReport]) -> Self {
        let mut columns = Self::COLUMNS;
        for r in reports {
            let name_cell = r.pkg.as_ref().map_or(PLACEHOLDER, |p| p.name.as_str());
            let version_cell = placeholder(r.pkg.as_ref().and_then(|p| p.version.as_deref()));
            let lens = [
                r.action.label().chars().count(),
                placeholder(r.id.as_deref()).chars().count(),
                version_cell.chars().count(),
                name_cell.chars().count(),
                r.detail.as_deref().unwrap_or("").chars().count(),
            ];
            for (col, len) in columns.iter_mut().zip(lens) {
                col.width = col.width.max(len);
            }
        }
        ColumnWidths(columns)
    }
}

/// Show a placeholder for unknown values.
fn placeholder(value: Option<&str>) -> &str {
    value.unwrap_or(PLACEHOLDER)
}

/// Join one cell per column into an aligned line.
///
/// Cells before the last are left-padded to their column width; the last
/// cell is appended as-is.
fn format_line(w: &ColumnWidths, cells: &[&str]) -> String {
    debug_assert_eq!(cells.len(), w.0.len());
    let mut line = String::new();
    for (i, (col, cell)) in w.0.iter().zip(cells).enumerate() {
        if i > 0 {
            line.push_str(SEPARATOR);
        }
        if i == w.0.len() - 1 {
            line.push_str(cell);
        } else {
            line.push_str(&format!("{cell:<width$}", width = col.width));
        }
    }
    line
}

/// Format the report table header line.
fn format_header(w: &ColumnWidths) -> String {
    format_line(w, &w.0.map(|col| col.header))
}

/// Format a separator line between the header and the entries.
fn format_separator(w: &ColumnWidths) -> String {
    let dashes = w.0.map(|col| "-".repeat(col.width));
    format_line(w, &dashes.each_ref().map(String::as_str))
}

/// Format a per-entry line as aligned columns, e.g.
/// `Deleting  key4nzh9W7I5GZrya36Wmv  3.02.0  my-pkg`.
fn format_entry(r: &EntryReport, w: &ColumnWidths) -> String {
    let cells = [
        r.action.label(),
        placeholder(r.id.as_deref()),
        placeholder(r.pkg.as_ref().and_then(|p| p.version.as_deref())),
        r.pkg.as_ref().map_or(PLACEHOLDER, |p| p.name.as_str()),
        r.detail.as_deref().unwrap_or(""),
    ];
    format_line(w, &cells)
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
        fs::hard_link(&metadata, &metadata.with_file_name("METADATA-link")).unwrap();
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

    #[test]
    fn parses_dist_info_name() {
        let pkg = Package::from("anyio-4.0.0");
        assert_eq!(pkg.name, "anyio");
        assert_eq!(pkg.version.as_deref(), Some("4.0.0"));

        // No version separator — treat the whole name as the package name.
        let odd = Package::from("odd");
        assert_eq!(odd.name, "odd");
        assert_eq!(odd.version, None);
    }

    #[test]
    fn entry_action_labels() {
        assert_eq!(EntryAction::Deleting.to_string(), "Deleting");
        assert_eq!(EntryAction::Keeping.label(), "Keeping");
        assert_eq!(EntryAction::Skipping.label(), "Skipping");
        assert_eq!(EntryAction::Failed.label(), "Failed");
    }

    #[test]
    fn report_lines_are_aligned() {
        let widths = ColumnWidths([
            Column {
                header: "A",
                width: 4,
            },
            Column {
                header: "B",
                width: 3,
            },
            Column {
                header: "C",
                width: 5,
            },
            Column {
                header: "D",
                width: 2,
            },
            Column {
                header: "E",
                width: 1,
            },
        ]);

        assert_eq!(
            format_line(&widths, &["x", "yz", "hello", "ab", "z"]),
            "x     yz   hello  ab  z"
        );
        assert_eq!(format_header(&widths), "A     B    C      D   E");
        assert_eq!(format_separator(&widths), "----  ---  -----  --  -");

        // Entry row pads unknown version/name with "-".
        let report = EntryReport::unreadable("boom".to_string());
        let line = format_entry(&report, &widths);
        assert!(line.contains("Failed"));
        assert!(line.contains("boom"));
    }
}
