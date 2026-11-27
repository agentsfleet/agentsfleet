#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::fs;
use std::path::{Path, PathBuf};

use cap_std::ambient_authority;
use cap_std::fs::Dir;

use super::{Snapshot, walk};

/// A workspace on disk and its handle.
fn workspace() -> (tempfile::TempDir, Dir) {
    let scratch = tempfile::tempdir().unwrap();
    let dir = Dir::open_ambient_dir(scratch.path(), ambient_authority()).unwrap();
    (scratch, dir)
}

/// What a walk of `dir` keeps, read against nothing.
fn snapshot(dir: &Dir) -> Snapshot {
    walk(dir, None).unwrap().snapshot
}

fn paths(found: &[PathBuf]) -> Vec<&str> {
    let mut paths: Vec<_> = found.iter().map(|path| path.to_str().unwrap()).collect();
    paths.sort_unstable();
    paths
}

#[test]
fn a_walk_against_nothing_reads_every_file() {
    let (root, dir) = workspace();
    fs::create_dir(root.path().join("a")).unwrap();
    fs::write(root.path().join("a/one"), b"12345").unwrap();
    fs::write(root.path().join("two"), b"123").unwrap();

    let walked = walk(&dir, None).unwrap();

    assert_eq!(walked.entries, 3);
    assert_eq!(walked.changed_bytes, 8);
    assert_eq!(walked.removed, Vec::<PathBuf>::new());
}

#[test]
fn an_untouched_tree_changes_nothing() {
    let (root, dir) = workspace();
    fs::create_dir(root.path().join("a")).unwrap();
    fs::write(root.path().join("a/one"), b"one").unwrap();
    let mut before = snapshot(&dir);
    // The clock moved on since the files were written.
    before.tick = (i64::MAX, 0);

    let walked = walk(&dir, Some(&before)).unwrap();

    assert!(walked.roots.is_empty(), "{:?}", walked.roots);
    assert_eq!(walked.changed_bytes, 0);
    assert_eq!(walked.entries, 2);
}

#[test]
fn a_new_directory_is_one_root_read_whole() {
    let (root, dir) = workspace();
    let before = snapshot(&dir);
    fs::create_dir_all(root.path().join("fresh/deeper")).unwrap();
    fs::write(root.path().join("fresh/deeper/file"), b"abcd").unwrap();
    fs::write(root.path().join("fresh/file"), b"ab").unwrap();

    let walked = walk(&dir, Some(&before)).unwrap();

    assert_eq!(paths(&walked.roots), ["fresh"]);
    assert_eq!(walked.changed_bytes, 6);
    assert_eq!(walked.snapshot.seen.len(), 4, "every entry below is kept");
}

#[test]
fn removed_paths_are_listed_once_at_their_highest_gone_directory() {
    let (root, dir) = workspace();
    fs::create_dir_all(root.path().join("gone/below")).unwrap();
    fs::write(root.path().join("gone/below/file"), b"x").unwrap();
    fs::create_dir(root.path().join("kept")).unwrap();
    fs::write(root.path().join("kept/dropped"), b"x").unwrap();
    fs::write(root.path().join("kept/stays"), b"x").unwrap();
    let before = snapshot(&dir);
    fs::remove_dir_all(root.path().join("gone")).unwrap();
    fs::remove_file(root.path().join("kept/dropped")).unwrap();

    let walked = walk(&dir, Some(&before)).unwrap();

    assert_eq!(paths(&walked.removed), ["gone", "kept/dropped"]);
}

#[test]
fn an_entry_that_changed_kind_is_changed_and_its_old_children_removed() {
    let (root, dir) = workspace();
    fs::create_dir(root.path().join("was_directory")).unwrap();
    fs::write(root.path().join("was_directory/child"), b"x").unwrap();
    fs::write(root.path().join("was_file"), b"x").unwrap();
    let before = snapshot(&dir);
    fs::remove_dir_all(root.path().join("was_directory")).unwrap();
    fs::write(root.path().join("was_directory"), b"now a file").unwrap();
    fs::remove_file(root.path().join("was_file")).unwrap();
    fs::create_dir(root.path().join("was_file")).unwrap();
    fs::write(root.path().join("was_file/inside"), b"y").unwrap();

    let walked = walk(&dir, Some(&before)).unwrap();

    assert_eq!(paths(&walked.roots), ["was_directory", "was_file"]);
    assert_eq!(paths(&walked.removed), ["was_directory/child"]);
}

/// A rewrite inside the previous pin's clock tick, at the same size, with
/// its modification time set back, can leave every stat field the cache
/// holds as it was. The guard reads an entry whose change time is at or
/// after that tick again; one before it is trusted. This fixes the form:
/// `changed >= tick`.
#[test]
fn a_same_tick_same_size_rewrite_is_saved() {
    let (root, dir) = workspace();
    let file = root.path().join("notes");
    fs::write(&file, b"before").unwrap();
    let written = fs::metadata(&file).unwrap().modified().unwrap();
    let mut before = snapshot(&dir);
    fs::write(&file, b"after!").unwrap();
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(written)
        .unwrap();
    // A coarse clock would have stamped the rewrite as the cache saw it.
    let now = snapshot(&dir).seen.remove(Path::new("notes")).unwrap();
    before.seen.insert(PathBuf::from("notes"), now);

    before.tick = now.identity.changed;
    let at_the_tick = walk(&dir, Some(&before)).unwrap();
    before.tick = (now.identity.changed.0, now.identity.changed.1 + 1);
    let after_the_tick = walk(&dir, Some(&before)).unwrap();

    assert_eq!(
        paths(&at_the_tick.roots),
        ["notes"],
        "read again at the tick"
    );
    assert_eq!(at_the_tick.changed_bytes, 6);
    assert!(after_the_tick.roots.is_empty(), "trusted before the tick");
}

#[test]
fn a_rewritten_file_and_a_repointed_link_are_changed() {
    let (root, dir) = workspace();
    fs::write(root.path().join("file"), b"one").unwrap();
    std::os::unix::fs::symlink("file", root.path().join("link")).unwrap();
    fs::write(root.path().join("still"), b"same").unwrap();
    let mut before = snapshot(&dir);
    before.tick = (i64::MAX, 0);
    fs::write(root.path().join("file"), b"three").unwrap();
    fs::remove_file(root.path().join("link")).unwrap();
    std::os::unix::fs::symlink("still", root.path().join("link")).unwrap();

    let walked = walk(&dir, Some(&before)).unwrap();

    assert_eq!(paths(&walked.roots), ["file", "link"]);
    assert_eq!(walked.changed_bytes, 5);
}
