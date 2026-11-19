//! The expanded base plus overlay: a removal takes its descendants, the counts
//! must agree, a file never parents a path, and links resolve within bounds.

use af_wire::task::{CheckpointEntry, CheckpointFile};

use super::Tree;
use crate::{Result, error::ErrorKind, task::TaskRefusal};

fn file(path: &str, size: u64) -> CheckpointEntry {
    CheckpointEntry::File(CheckpointFile {
        path: path.to_owned(),
        executable: false,
        size,
        objects: Vec::new(),
    })
}

fn directory(path: &str) -> CheckpointEntry {
    CheckpointEntry::Directory {
        path: path.to_owned(),
    }
}

fn link(path: &str, target: &str) -> CheckpointEntry {
    CheckpointEntry::Symlink {
        path: path.to_owned(),
        target: target.to_owned(),
    }
}

/// The refusal a result carries, `Err(None)` for any failure that is not one.
fn outcome(result: Result<()>) -> core::result::Result<(), Option<TaskRefusal>> {
    result.map_err(|error| match error.kind() {
        ErrorKind::TaskRefused { reason } => Some(*reason),
        _ => None,
    })
}

/// Every entry applied in order; each application must succeed.
fn applied(entries: Vec<CheckpointEntry>) -> Tree {
    let mut tree = Tree::default();
    for entry in entries {
        let path = entry.path().to_owned();
        assert_eq!(outcome(tree.apply(entry)), Ok(()), "{path}");
    }
    tree
}

#[test]
fn a_removal_takes_the_entry_and_its_descendants_but_not_a_prefixed_sibling() {
    let tree = applied(vec![
        directory("d"),
        file("d/f", 3),
        file("d/g/h", 2),
        file("d2", 1),
        file("e", 5),
        CheckpointEntry::Remove {
            path: "d".to_owned(),
        },
    ]);

    assert_eq!(outcome(tree.verify(2, 6)), Ok(()));
    assert_eq!(
        outcome(tree.verify(3, 6)),
        Err(Some(TaskRefusal::CheckpointIncompatible)),
        "the removed entries no longer count"
    );
    assert_eq!(
        outcome(tree.verify(2, 11)),
        Err(Some(TaskRefusal::CheckpointIncompatible)),
        "the removed bytes no longer count"
    );
}

#[test]
fn a_file_or_link_never_parents_another_path() {
    let under_directory = applied(vec![directory("a"), file("a/b", 1)]);
    let under_file = applied(vec![file("a", 1), file("a/b", 1)]);
    let under_link = applied(vec![link("l", "d"), directory("d"), file("l/x", 1)]);

    assert_eq!(outcome(under_directory.verify(2, 1)), Ok(()));
    assert_eq!(
        outcome(under_file.verify(2, 2)),
        Err(Some(TaskRefusal::CheckpointIncompatible))
    );
    assert_eq!(
        outcome(under_link.verify(3, 1)),
        Err(Some(TaskRefusal::CheckpointIncompatible))
    );
}

/// `a` resolves through `b`, so `b` is already resolved when the walk reaches
/// it; empty and `.` components change nothing.
#[test]
fn a_link_through_another_link_resolves() {
    let tree = applied(vec![
        link("a", "./b//f"),
        link("b", "d"),
        directory("d"),
        file("d/f", 3),
    ]);

    assert_eq!(outcome(tree.verify(4, 3)), Ok(()));
}

#[test]
fn a_link_cycle_is_incompatible() {
    let tree = applied(vec![link("a", "b"), link("b", "a")]);

    assert_eq!(
        outcome(tree.verify(2, 0)),
        Err(Some(TaskRefusal::CheckpointIncompatible))
    );
}

/// `a` counts one substitution for itself and one for every pass through `s`:
/// thirty-nine passes reach the forty-link bound, forty pass it.
#[test]
fn a_target_past_forty_substitutions_is_incompatible() {
    let within = applied(vec![link("a", &["s"; 39].join("/")), link("s", ".")]);
    let past = applied(vec![link("a", &["s"; 40].join("/")), link("s", ".")]);

    assert_eq!(outcome(within.verify(2, 0)), Ok(()));
    assert_eq!(
        outcome(past.verify(2, 0)),
        Err(Some(TaskRefusal::CheckpointIncompatible))
    );
}
