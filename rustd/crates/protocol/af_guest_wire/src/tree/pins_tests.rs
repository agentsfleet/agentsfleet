#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::fs;
use std::path::PathBuf;

use cap_std::ambient_authority;
use cap_std::fs::Dir;

use super::Pins;
use crate::protocol::MAX_FRAME_BYTES;
use jsonrpsee_types::error::INVALID_PARAMS_CODE;

fn workspace() -> (tempfile::TempDir, Dir) {
    let scratch = tempfile::tempdir().unwrap();
    let dir = Dir::open_ambient_dir(scratch.path(), ambient_authority()).unwrap();
    (scratch, dir)
}

/// Pins whose committed snapshot is `generation`'s, taken of `dir` now, its
/// tick moved past every write so far.
fn committed(dir: &Dir, generation: &str) -> Pins {
    let mut pins = Pins::default();
    let first = pins.walk(dir, None, MAX_FRAME_BYTES).unwrap();
    pins.release(first.pin, Some(generation)).unwrap();
    if let Some((_, snapshot)) = pins.committed.as_mut() {
        snapshot.tick = (i64::MAX, 0);
    }
    pins
}

#[test]
fn a_walk_against_the_committed_generation_is_a_delta() {
    let (root, dir) = workspace();
    fs::write(root.path().join("a"), b"a").unwrap();
    fs::write(root.path().join("b"), b"b").unwrap();
    let mut pins = committed(&dir, "g1");
    fs::write(root.path().join("a"), b"aa").unwrap();
    fs::remove_file(root.path().join("b")).unwrap();

    let walked = pins.walk(&dir, Some("g1"), MAX_FRAME_BYTES).unwrap();

    assert!(walked.delta);
    assert_eq!(walked.roots, [PathBuf::from("a")]);
    assert_eq!(walked.removed, ["b"]);
    assert_eq!(walked.changed_bytes, 2);
}

#[test]
fn a_walk_against_another_generation_reads_everything() {
    let (root, dir) = workspace();
    fs::write(root.path().join("a"), b"abc").unwrap();
    let mut pins = committed(&dir, "g1");

    for parent in [Some("g2"), None] {
        let walked = pins.walk(&dir, parent, MAX_FRAME_BYTES).unwrap();
        assert!(!walked.delta);
        assert_eq!(walked.roots, [PathBuf::from(".")]);
        assert_eq!(walked.removed, Vec::<String>::new());
        assert_eq!(walked.changed_bytes, 3);
    }
}

#[test]
fn a_removed_list_past_the_frame_makes_the_walk_whole() {
    let (root, dir) = workspace();
    for index in 0..64 {
        fs::write(root.path().join(format!("file-{index:04}")), b"x").unwrap();
    }
    let mut pins = committed(&dir, "g1");
    for index in 0..64 {
        fs::remove_file(root.path().join(format!("file-{index:04}"))).unwrap();
    }

    // Half the frame, as three quarters, carries the answer's list: 64
    // names of 9 bytes, quoted and separated, do not fit a 1 KiB frame's 384.
    let walked = pins.walk(&dir, Some("g1"), 1024).unwrap();

    assert!(!walked.delta);
    assert_eq!(walked.removed, Vec::<String>::new());
}

#[test]
fn a_release_without_a_generation_keeps_the_committed_snapshot() {
    let (root, dir) = workspace();
    fs::write(root.path().join("a"), b"a").unwrap();
    let mut pins = committed(&dir, "g1");
    fs::write(root.path().join("a"), b"changed").unwrap();
    let unpublished = pins.walk(&dir, Some("g1"), MAX_FRAME_BYTES).unwrap();

    pins.release(unpublished.pin, None).unwrap();
    let again = pins.walk(&dir, Some("g1"), MAX_FRAME_BYTES).unwrap();

    assert!(again.delta, "g1's snapshot still stands");
    assert_eq!(again.roots, [PathBuf::from("a")], "still changed since g1");
}

#[test]
fn a_committed_release_labels_the_next_parent() {
    let (root, dir) = workspace();
    fs::write(root.path().join("a"), b"a").unwrap();
    let mut pins = committed(&dir, "g1");
    let second = pins.walk(&dir, Some("g1"), MAX_FRAME_BYTES).unwrap();

    pins.release(second.pin, Some("g2")).unwrap();

    assert!(!pins.walk(&dir, Some("g1"), MAX_FRAME_BYTES).unwrap().delta);
    assert!(pins.walk(&dir, Some("g2"), MAX_FRAME_BYTES).unwrap().delta);
}

#[test]
fn a_release_of_an_unknown_pin_is_refused() {
    let (_root, dir) = workspace();
    let mut pins = committed(&dir, "g1");

    let refused = pins.release(99, Some("g2")).unwrap_err();

    assert_eq!(refused.rpc_code(), INVALID_PARAMS_CODE);
    assert!(pins.release(99, None).is_err());
}
