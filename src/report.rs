use std::fmt;

use crate::package::Package;

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

/// The action taken for an entry, shown in the report.
#[derive(Debug)]
pub enum EntryAction {
    Deleting,
    Keeping,
    Skipping,
    Failed,
}

impl EntryAction {
    /// The action label shown in the report.
    pub fn label(&self) -> &'static str {
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
pub struct EntryReport {
    pub level: log::Level,
    pub action: EntryAction,
    /// Archive id (the uv cache directory name), if known.
    pub id: Option<String>,
    /// Package identity, if a `.dist-info` directory was found.
    pub pkg: Option<Package>,
    /// Supplementary detail, e.g. a reason, if any.
    pub detail: Option<String>,
    pub result: PruneResult,
}

impl EntryReport {
    /// Entry kept because it is still hardlinked in another environment.
    pub fn kept(id: String, pkg: Package) -> Self {
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
    pub fn skipped(id: String, detail: impl Into<String>, no_dist_info: bool) -> Self {
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
    pub fn deleting(
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
    pub fn failed(id: String, pkg: Option<Package>, detail: String) -> Self {
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
    pub fn unreadable(detail: String) -> Self {
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

/// Sort, print and summarize the per-entry reports.
pub fn report_entries(mut reports: Vec<EntryReport>, dry_run: bool) -> PruneResult {
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

#[cfg(test)]
mod tests {
    use super::*;

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
