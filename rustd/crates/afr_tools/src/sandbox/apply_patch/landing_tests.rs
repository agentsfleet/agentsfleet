#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

//! How a patch lands: hunk after hunk on one path, moves, and a write the
//! executor refuses partway.

use super::tests::{KEEP, apply, planted};
use crate::runtime::ToolErrorCode;
use crate::testing::Live;

/// A write the executor refuses while hunks land is reported in the
/// executor's words, and what landed before it stays: the patch is not
/// transactional past its first write, and the answer says which it was.
#[tokio::test]
async fn a_write_refused_while_landing_reports_the_executor_and_keeps_what_landed() {
    if rustix::process::geteuid().is_root() {
        // Root writes through mode bits, so there is no refusal to inject.
        return;
    }
    let live = Live::start().await;
    let locked = live.root.join("kept.txt");
    std::fs::write(&locked, "a\n").unwrap();
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o444)).unwrap();
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Add File: landed.txt\n",
        "+first\n",
        "*** Update File: kept.txt\n",
        "@@\n",
        "-a\n",
        "+b\n",
        "*** End Patch\n",
    );

    let refused = apply(&live, patch).await;
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();

    assert_eq!(
        refused.error_code,
        Some(ToolErrorCode::SandboxUnavailable),
        "{refused:?}"
    );
    assert!(
        refused.text.contains("ermission denied"),
        "{}",
        refused.text
    );
    assert_eq!(
        std::fs::read_to_string(live.root.join("landed.txt")).unwrap(),
        "first\n",
        "the add hunk landed before the refused write"
    );
    assert_eq!(std::fs::read_to_string(&locked).unwrap(), "a\n");
    live.stop().await;
}

/// A move writes the new path, then removes the old; a delete the executor
/// refuses leaves both files, says so, and a retry heals it.
#[tokio::test]
async fn a_move_whose_delete_is_refused_keeps_both_files_and_a_retry_heals_it() {
    if rustix::process::geteuid().is_root() {
        // Root unlinks through mode bits, so there is no refusal to inject.
        return;
    }
    let live = Live::start().await;
    let locked = live.root.join("ro");
    std::fs::create_dir(&locked).unwrap();
    std::fs::write(locked.join("old.txt"), "a\n").unwrap();
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o555)).unwrap();
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: ro/old.txt\n",
        "*** Move to: moved.txt\n",
        "@@\n",
        "-a\n",
        "+b\n",
        "*** End Patch\n",
    );

    let refused = apply(&live, patch).await;
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let healed = apply(&live, patch).await;

    assert_eq!(
        refused.error_code,
        Some(ToolErrorCode::SandboxUnavailable),
        "{refused:?}"
    );
    assert!(
        refused.text.contains("ermission denied"),
        "{}",
        refused.text
    );
    assert_eq!(healed.error_code, None, "{healed:?}");
    assert_eq!(
        std::fs::read_to_string(live.root.join("moved.txt")).unwrap(),
        "b\n"
    );
    assert!(
        !locked.join("old.txt").exists(),
        "the retry removed the old path"
    );
    live.stop().await;
}

/// Two hunks on one file land one after the other, as Codex applies them:
/// the second starts from what the first left, so neither change is lost.
#[tokio::test]
async fn two_update_hunks_on_one_file_both_land() {
    let live = planted().await;
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: src/keep.txt\n",
        "@@\n",
        "-one\n",
        "+ONE\n",
        "*** Update File: ./src/keep.txt\n",
        "@@\n",
        "-three\n",
        "+THREE\n",
        "*** End Patch\n",
    );

    let applied = apply(&live, patch).await;

    assert_eq!(applied.error_code, None, "{applied:?}");
    assert_eq!(
        std::fs::read_to_string(live.root.join(KEEP)).unwrap(),
        "ONE\ntwo\nTHREE\n"
    );
    assert!(applied.text.ends_with("+2 \u{2212}2"), "{}", applied.text);
    live.stop().await;
}

/// A move to the path the file already has is an update: the file stays,
/// changed, and is not deleted after its write.
#[tokio::test]
async fn a_move_to_its_own_path_keeps_the_file() {
    let live = planted().await;
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: src/keep.txt\n",
        "*** Move to: /workspace/./src/keep.txt\n",
        "@@\n",
        "-two\n",
        "+TWO\n",
        "*** End Patch\n",
    );

    let applied = apply(&live, patch).await;

    assert_eq!(applied.error_code, None, "{applied:?}");
    assert_eq!(
        std::fs::read_to_string(live.root.join(KEEP)).unwrap(),
        "one\nTWO\nthree\n"
    );
    live.stop().await;
}

/// A hunk on a path an earlier hunk of the same patch moved away or deleted
/// reads `file_not_found`, and nothing of the patch lands.
#[tokio::test]
async fn a_hunk_on_a_path_the_patch_already_removed_lands_nothing() {
    let live = planted().await;
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: src/keep.txt\n",
        "*** Move to: src/kept.txt\n",
        "@@\n",
        "-two\n",
        "+TWO\n",
        "*** Delete File: src/old.txt\n",
        "*** Update File: src/keep.txt\n",
        "@@\n",
        "-one\n",
        "+ONE\n",
        "*** End Patch\n",
    );

    let refused = apply(&live, patch).await;

    assert_eq!(
        refused.error_code,
        Some(ToolErrorCode::FileNotFound),
        "{refused:?}"
    );
    assert!(refused.text.contains("src/keep.txt"), "{}", refused.text);
    assert_eq!(
        std::fs::read_to_string(live.root.join(KEEP)).unwrap(),
        "one\ntwo\nthree\n"
    );
    assert!(!live.root.join("src/kept.txt").exists());
    assert!(live.root.join("src/old.txt").exists());
    live.stop().await;
}

/// A file moved by one hunk is updated by the next under its new name.
#[tokio::test]
async fn a_moved_file_is_updated_under_its_new_name() {
    let live = planted().await;
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: src/keep.txt\n",
        "*** Move to: src/kept.txt\n",
        "@@\n",
        "-two\n",
        "+TWO\n",
        "*** Update File: src/kept.txt\n",
        "@@\n",
        "-three\n",
        "+THREE\n",
        "*** End Patch\n",
    );

    let applied = apply(&live, patch).await;

    assert_eq!(applied.error_code, None, "{applied:?}");
    assert_eq!(
        std::fs::read_to_string(live.root.join("src/kept.txt")).unwrap(),
        "one\nTWO\nTHREE\n"
    );
    assert!(!live.root.join(KEEP).exists());
    live.stop().await;
}
