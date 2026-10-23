//! Three-way comparison of the stored base, the database projection and the
//! workspace files.
//!
//! Each path is compared on its exact bytes. A side that matches the base is
//! unchanged; a side that differs has added, modified or removed the file. The
//! comparison only classifies paths. Callers decide what to import, export or
//! refuse.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use arcl_store::WorkspaceBaseFile;
use serde::Serialize;

use super::WorkspaceFile;

/// How one side's copy of a path differs from the stored base.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Change {
    /// The path is absent from the base and present on this side.
    Added,
    /// The path is present in the base and on this side with different bytes.
    Modified,
    /// The path is present in the base and absent from this side.
    Removed,
}

/// The three-way state of one workspace path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordState {
    /// The database and the workspace both match the base.
    Unchanged,
    /// The database differs from the base and the workspace matches it.
    DatabaseChanged(Change),
    /// The workspace differs from the base and the database matches it.
    WorkspaceChanged(Change),
    /// Both sides made the same change; only the base is out of date.
    Converged(Change),
    /// Both sides changed the path differently from the base.
    Divergent { database: Change, workspace: Change },
}

impl RecordState {
    /// Return whether the database and the workspace disagree about this path
    /// in a way that neither side can resolve on its own.
    pub fn is_divergent(self) -> bool {
        matches!(self, Self::Divergent { .. })
    }
}

/// The state of one path in a comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordComparison {
    /// The path relative to the workspace root.
    pub path: PathBuf,
    /// How the database and the workspace differ from the base at this path.
    pub state: RecordState,
}

/// The state of every path seen in the base, the database or the workspace.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Comparison {
    /// One entry per path, sorted by path.
    pub records: Vec<RecordComparison>,
}

impl Comparison {
    /// Return whether every path matches the base on both sides.
    pub fn is_clean(&self) -> bool {
        self.records.iter().all(|record| record.state == RecordState::Unchanged)
    }

    /// Return the paths whose sides changed differently from the base.
    pub fn divergent(&self) -> impl Iterator<Item = &RecordComparison> {
        self.records.iter().filter(|record| record.state.is_divergent())
    }
}

/// Classify every path against the stored base, the database projection and
/// the workspace files.
///
/// A path missing from a side counts as absent there. With an empty base, a
/// path both sides added with the same bytes converges and a path they added
/// with different bytes diverges.
pub fn compare(base: &[WorkspaceBaseFile], database: &[WorkspaceFile], workspace: &[WorkspaceFile]) -> Comparison {
    let base = base
        .iter()
        .map(|file| (PathBuf::from(&file.path), file.content.as_slice()))
        .collect::<BTreeMap<_, _>>();
    let database = by_path(database);
    let workspace = by_path(workspace);
    let paths = base
        .keys()
        .chain(database.keys())
        .chain(workspace.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    let records = paths
        .into_iter()
        .map(|path| {
            let state = classify(
                base.get(&path).copied(),
                database.get(&path).copied(),
                workspace.get(&path).copied(),
            );
            RecordComparison { path, state }
        })
        .collect();
    Comparison { records }
}

fn by_path(files: &[WorkspaceFile]) -> BTreeMap<PathBuf, &[u8]> {
    files
        .iter()
        .map(|file| (file.path.clone(), file.content.as_slice()))
        .collect()
}

fn classify(base: Option<&[u8]>, database: Option<&[u8]>, workspace: Option<&[u8]>) -> RecordState {
    match (change(base, database), change(base, workspace)) {
        (None, None) => RecordState::Unchanged,
        (Some(change), None) => RecordState::DatabaseChanged(change),
        (None, Some(change)) => RecordState::WorkspaceChanged(change),
        (Some(change), Some(_)) if database == workspace => RecordState::Converged(change),
        (Some(database), Some(workspace)) => RecordState::Divergent { database, workspace },
    }
}

fn change(base: Option<&[u8]>, side: Option<&[u8]>) -> Option<Change> {
    match (base, side) {
        (None, None) => None,
        (None, Some(_)) => Some(Change::Added),
        (Some(_), None) => Some(Change::Removed),
        (Some(base), Some(side)) if base == side => None,
        (Some(_), Some(_)) => Some(Change::Modified),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: &str = "tasks/arcl-t-01K0B31M6VGK4YH8VKT4C0D2DR.md";

    fn base(content: Option<&str>) -> Vec<WorkspaceBaseFile> {
        content
            .map(|content| WorkspaceBaseFile { path: PATH.to_owned(), content: content.as_bytes().to_vec() })
            .into_iter()
            .collect()
    }

    fn side(content: Option<&str>) -> Vec<WorkspaceFile> {
        content
            .map(|content| WorkspaceFile { path: PathBuf::from(PATH), content: content.as_bytes().to_vec() })
            .into_iter()
            .collect()
    }

    fn state(b: Option<&str>, d: Option<&str>, w: Option<&str>) -> RecordState {
        let comparison = compare(&base(b), &side(d), &side(w));
        match comparison.records.as_slice() {
            [record] => record.state,
            [] => RecordState::Unchanged,
            records => panic!("one path produced {} records", records.len()),
        }
    }

    #[test]
    fn unchanged_when_both_sides_match_the_base() {
        assert_eq!(state(Some("a"), Some("a"), Some("a")), RecordState::Unchanged);
        assert_eq!(state(None, None, None), RecordState::Unchanged);
    }

    #[test]
    fn database_only_changes_are_exports() {
        assert_eq!(
            state(None, Some("a"), None),
            RecordState::DatabaseChanged(Change::Added)
        );
        assert_eq!(
            state(Some("a"), Some("b"), Some("a")),
            RecordState::DatabaseChanged(Change::Modified)
        );
        assert_eq!(
            state(Some("a"), None, Some("a")),
            RecordState::DatabaseChanged(Change::Removed)
        );
    }

    #[test]
    fn workspace_only_changes_are_imports() {
        assert_eq!(
            state(None, None, Some("a")),
            RecordState::WorkspaceChanged(Change::Added)
        );
        assert_eq!(
            state(Some("a"), Some("a"), Some("b")),
            RecordState::WorkspaceChanged(Change::Modified)
        );
        assert_eq!(
            state(Some("a"), Some("a"), None),
            RecordState::WorkspaceChanged(Change::Removed)
        );
    }

    #[test]
    fn identical_changes_on_both_sides_converge() {
        assert_eq!(state(None, Some("a"), Some("a")), RecordState::Converged(Change::Added));
        assert_eq!(
            state(Some("a"), Some("b"), Some("b")),
            RecordState::Converged(Change::Modified)
        );
        assert_eq!(state(Some("a"), None, None), RecordState::Converged(Change::Removed));
    }

    #[test]
    fn different_changes_on_both_sides_diverge() {
        let cases = [
            (None, Some("a"), Some("b"), Change::Added, Change::Added),
            (Some("a"), Some("b"), Some("c"), Change::Modified, Change::Modified),
            (Some("a"), Some("b"), None, Change::Modified, Change::Removed),
            (Some("a"), None, Some("b"), Change::Removed, Change::Modified),
        ];
        for (b, d, w, database, workspace) in cases {
            assert_eq!(
                state(b, d, w),
                RecordState::Divergent { database, workspace },
                "base {b:?}, database {d:?}, workspace {w:?}"
            );
        }
    }

    #[test]
    fn every_combination_matches_the_definitions() {
        let values = [None, Some("a"), Some("b"), Some("c")];
        for b in values {
            for d in values {
                for w in values {
                    let got = state(b, d, w);
                    let database_changed = d != b;
                    let workspace_changed = w != b;
                    let expected_kind = match got {
                        RecordState::Unchanged => !database_changed && !workspace_changed,
                        RecordState::DatabaseChanged(_) => database_changed && !workspace_changed,
                        RecordState::WorkspaceChanged(_) => !database_changed && workspace_changed,
                        RecordState::Converged(_) => database_changed && workspace_changed && d == w,
                        RecordState::Divergent { .. } => database_changed && workspace_changed && d != w,
                    };
                    assert!(
                        expected_kind,
                        "base {b:?}, database {d:?}, workspace {w:?} gave {got:?}"
                    );

                    let mirrored = match got {
                        RecordState::DatabaseChanged(change) => RecordState::WorkspaceChanged(change),
                        RecordState::WorkspaceChanged(change) => RecordState::DatabaseChanged(change),
                        RecordState::Divergent { database, workspace } => {
                            RecordState::Divergent { database: workspace, workspace: database }
                        }
                        other => other,
                    };
                    assert_eq!(mirrored, state(b, w, d), "swapping sides must mirror the state");
                }
            }
        }
    }

    #[test]
    fn comparison_covers_every_path_in_order() {
        let base = vec![
            WorkspaceBaseFile { path: "manifest.toml".to_owned(), content: b"m".to_vec() },
            WorkspaceBaseFile { path: "notes/b.md".to_owned(), content: b"b".to_vec() },
        ];
        let database = vec![
            WorkspaceFile { path: PathBuf::from("manifest.toml"), content: b"m".to_vec() },
            WorkspaceFile { path: PathBuf::from("notes/b.md"), content: b"b2".to_vec() },
            WorkspaceFile { path: PathBuf::from("captures/a.md"), content: b"a".to_vec() },
        ];
        let workspace = vec![
            WorkspaceFile { path: PathBuf::from("manifest.toml"), content: b"m".to_vec() },
            WorkspaceFile { path: PathBuf::from("notes/b.md"), content: b"b3".to_vec() },
            WorkspaceFile { path: PathBuf::from("specs/c.md"), content: b"c".to_vec() },
        ];

        let comparison = compare(&base, &database, &workspace);
        let paths = comparison
            .records
            .iter()
            .map(|record| record.path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(paths, ["captures/a.md", "manifest.toml", "notes/b.md", "specs/c.md"]);
        assert_eq!(comparison.records[0].state, RecordState::DatabaseChanged(Change::Added));
        assert_eq!(comparison.records[1].state, RecordState::Unchanged);
        assert_eq!(
            comparison.records[3].state,
            RecordState::WorkspaceChanged(Change::Added)
        );
        assert!(!comparison.is_clean());
        let divergent = comparison.divergent().map(|record| &record.path).collect::<Vec<_>>();
        assert_eq!(divergent, [&PathBuf::from("notes/b.md")]);
    }

    #[test]
    fn matching_sides_are_clean() {
        let base = vec![WorkspaceBaseFile { path: "manifest.toml".to_owned(), content: b"m".to_vec() }];
        let files = vec![WorkspaceFile { path: PathBuf::from("manifest.toml"), content: b"m".to_vec() }];
        let comparison = compare(&base, &files, &files);
        assert!(comparison.is_clean());
        assert_eq!(comparison.divergent().count(), 0);
    }
}
