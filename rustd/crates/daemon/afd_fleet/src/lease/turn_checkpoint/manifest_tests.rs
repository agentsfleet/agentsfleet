//! A full manifest lists each entry once, in path order across pages, under a
//! directory it has already listed, and never as a removal.

use af_wire::task::{CheckpointEntry, CheckpointFile};

use super::Contents;
use crate::{Result, error::ErrorKind, lease::turn_checkpoint::TurnCheckpointRefusal};

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

/// The refusal a result carries, `Err(None)` for any failure that is not one.
fn outcome(result: Result<()>) -> core::result::Result<(), Option<TurnCheckpointRefusal>> {
    result.map_err(|error| match error.kind() {
        ErrorKind::TurnCheckpointRefused { reason } => Some(*reason),
        _ => None,
    })
}

#[test]
fn an_ordered_manifest_counts_its_entries_bytes_directories_and_links() {
    let mut contents = Contents::default();
    let entries = [
        directory("d"),
        file("d/f", 3),
        CheckpointEntry::Symlink {
            path: "d/l".to_owned(),
            target: "f".to_owned(),
        },
    ];

    assert_eq!(outcome(contents.append(&entries)), Ok(()));
    assert_eq!(contents.entries, 3);
    assert_eq!(contents.bytes, 3);
    assert!(contents.directories.contains("d"));
    assert_eq!(contents.links.get("d/l").map(String::as_str), Some("f"));
    assert_eq!(contents.previous, "d/l");
}

#[test]
fn an_entry_out_of_order_or_repeated_is_incompatible() {
    for entries in [[file("b", 1), file("a", 1)], [file("a", 1), file("a", 1)]] {
        assert_eq!(
            outcome(Contents::default().append(&entries)),
            Err(Some(TurnCheckpointRefusal::Incompatible))
        );
    }
}

/// Order runs across pages: a later page cannot list a path before the last
/// path of the page before it.
#[test]
fn a_later_page_continues_the_earlier_pages_order() {
    let mut contents = Contents::default();

    assert_eq!(outcome(contents.append(&[directory("d")])), Ok(()));
    assert_eq!(
        outcome(contents.append(&[file("c", 1)])),
        Err(Some(TurnCheckpointRefusal::Incompatible))
    );
}

#[test]
fn an_entry_under_an_unlisted_directory_or_a_file_is_incompatible() {
    for entries in [vec![file("d/f", 1)], vec![file("d", 1), file("d/f", 1)]] {
        assert_eq!(
            outcome(Contents::default().append(&entries)),
            Err(Some(TurnCheckpointRefusal::Incompatible))
        );
    }
}

#[test]
fn a_removal_is_incompatible_in_a_full_manifest() {
    let entries = [CheckpointEntry::Remove {
        path: "a".to_owned(),
    }];

    assert_eq!(
        outcome(Contents::default().append(&entries)),
        Err(Some(TurnCheckpointRefusal::Incompatible))
    );
}
