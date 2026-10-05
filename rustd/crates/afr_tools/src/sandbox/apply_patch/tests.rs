#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use serde_json::json;

use crate::catalog::APPLY_PATCH;
use crate::lease::Lease;
use crate::runtime::{ToolErrorCode, ToolOutput};
use crate::sandbox::ScriptedExecutor;
use crate::testing::{Live, call_in, hosted, offered};

/// The argument name the calls spell.
const PATCH: &str = "patch";

/// The files the patches work on, and what they start as.
const KEEP: &str = "src/keep.txt";
const KEEP_TEXT: &str = "one\ntwo\nthree\n";
const OLD: &str = "src/old.txt";
const OLD_TEXT: &str = "bye\n";
/// A patch with one hunk of each kind: two lines added, one inserted after
/// a context line (whose leading space a line continuation would eat, so
/// the lines are joined by `concat!`), one file of one line deleted.
const EACH_KIND: &str = concat!(
    "*** Begin Patch\n",
    "*** Add File: src/new.txt\n",
    "+hello\n",
    "+world\n",
    "*** Update File: src/keep.txt\n",
    "@@\n",
    " two\n",
    "+two and a half\n",
    "*** Delete File: src/old.txt\n",
    "*** End Patch\n",
);

/// Applies `patch` through `live`'s executor.
async fn apply(live: &Live, patch: &str) -> ToolOutput {
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[APPLY_PATCH.name()]).unwrap();
    let mut lease = Lease::default();
    call_in(
        offered(&selection, &APPLY_PATCH),
        &live.client,
        &mut lease,
        json!({PATCH: patch}),
    )
    .await
}

/// A workspace holding [`KEEP`] and [`OLD`].
async fn planted() -> Live {
    let live = Live::start().await;
    std::fs::create_dir(live.root.join("src")).unwrap();
    std::fs::write(live.root.join(KEEP), KEEP_TEXT).unwrap();
    std::fs::write(live.root.join(OLD), OLD_TEXT).unwrap();
    live
}

#[tokio::test]
async fn test_apply_patch_applies_codex_grammar() {
    let live = planted().await;

    let applied = apply(&live, EACH_KIND).await;

    assert_eq!(applied.error_code, None, "{applied:?}");
    assert_eq!(
        applied.text,
        "Success. Updated the following files:\nA src/new.txt\nM src/keep.txt\nD src/old.txt\n+3 \u{2212}1"
    );
    assert_eq!(
        std::fs::read_to_string(live.root.join("src/new.txt")).unwrap(),
        "hello\nworld\n"
    );
    assert_eq!(
        std::fs::read_to_string(live.root.join(KEEP)).unwrap(),
        "one\ntwo\ntwo and a half\nthree\n"
    );
    assert!(!live.root.join(OLD).exists());
    live.stop().await;
}

#[tokio::test]
async fn a_move_hunk_writes_the_new_path_and_removes_the_old() {
    let live = planted().await;
    let patch = "*** Begin Patch\n\
*** Update File: src/keep.txt\n\
*** Move to: src/kept.txt\n\
@@\n\
-two\n\
+TWO\n\
*** End Patch\n";

    let applied = apply(&live, patch).await;

    assert_eq!(applied.error_code, None, "{applied:?}");
    assert!(
        applied.text.contains("M src/kept.txt\n"),
        "{}",
        applied.text
    );
    assert!(applied.text.ends_with("+1 \u{2212}1"), "{}", applied.text);
    assert_eq!(
        std::fs::read_to_string(live.root.join("src/kept.txt")).unwrap(),
        "one\nTWO\nthree\n"
    );
    assert!(!live.root.join(KEEP).exists());
    live.stop().await;
}

#[tokio::test]
async fn a_files_line_endings_are_kept() {
    let live = Live::start().await;
    std::fs::write(live.root.join("dos.txt"), "a\r\nb\r\n").unwrap();
    let patch = "*** Begin Patch\n\
*** Update File: dos.txt\n\
@@\n\
-b\n\
+B\n\
+C\n\
*** End Patch\n";

    let applied = apply(&live, patch).await;

    assert_eq!(applied.error_code, None, "{applied:?}");
    assert_eq!(
        std::fs::read(live.root.join("dos.txt")).unwrap(),
        b"a\r\nB\r\nC\r\n"
    );
    live.stop().await;
}

#[tokio::test]
async fn a_patch_that_does_not_parse_or_changes_nothing_is_refused() {
    let live = Live::start().await;

    let unparsed = apply(&live, "*** Add File: x\n+y\n").await;
    let empty = apply(&live, "*** Begin Patch\n*** End Patch\n").await;

    assert_eq!(
        unparsed.error_code,
        Some(ToolErrorCode::PatchInvalid),
        "{unparsed:?}"
    );
    assert!(
        unparsed.text.contains("*** Begin Patch"),
        "{}",
        unparsed.text
    );
    assert_eq!(
        empty.error_code,
        Some(ToolErrorCode::PatchInvalid),
        "{empty:?}"
    );
    assert!(empty.text.contains("changes no file"), "{}", empty.text);
    assert_eq!(
        std::fs::read_dir(&live.root).unwrap().count(),
        0,
        "nothing was written"
    );
    live.stop().await;
}

/// Every hunk is computed before the first write: a later hunk that cannot
/// land keeps an earlier one from landing.
#[tokio::test]
async fn a_hunk_whose_lines_are_not_in_the_file_lands_nothing() {
    let live = planted().await;
    let patch = "*** Begin Patch\n\
*** Add File: src/fresh.txt\n\
+fresh\n\
*** Update File: src/keep.txt\n\
@@\n\
-nope\n\
+yes\n\
*** End Patch\n";

    let refused = apply(&live, patch).await;

    assert_eq!(
        refused.error_code,
        Some(ToolErrorCode::PatchInvalid),
        "{refused:?}"
    );
    assert!(
        refused
            .text
            .contains("Failed to find expected lines in src/keep.txt"),
        "{}",
        refused.text
    );
    assert!(
        !live.root.join("src/fresh.txt").exists(),
        "the add hunk did not land"
    );
    assert_eq!(
        std::fs::read_to_string(live.root.join(KEEP)).unwrap(),
        KEEP_TEXT
    );
    live.stop().await;
}

#[tokio::test]
async fn a_hunk_on_a_file_the_workspace_does_not_have_reads_file_not_found() {
    let live = Live::start().await;

    let update = apply(
        &live,
        "*** Begin Patch\n*** Update File: absent.txt\n@@\n-a\n+b\n*** End Patch\n",
    )
    .await;
    let delete = apply(
        &live,
        "*** Begin Patch\n*** Delete File: absent.txt\n*** End Patch\n",
    )
    .await;

    for refused in [&update, &delete] {
        assert_eq!(
            refused.error_code,
            Some(ToolErrorCode::FileNotFound),
            "{refused:?}"
        );
    }
    live.stop().await;
}

/// A hunk path that leaves the workspace is refused before any call: the
/// scripted executor refuses every file call, and no refusal of its shows.
#[tokio::test]
async fn a_hunk_path_leaving_the_workspace_is_refused_before_any_call() {
    let executor = ScriptedExecutor::default();
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[APPLY_PATCH.name()]).unwrap();
    let tool = offered(&selection, &APPLY_PATCH);
    let mut lease = Lease::default();

    for patch in [
        "*** Begin Patch\n*** Add File: ../escape.txt\n+x\n*** End Patch\n",
        "*** Begin Patch\n*** Add File: /etc/planted\n+x\n*** End Patch\n",
        "*** Begin Patch\n*** Delete File: a/../../b\n*** End Patch\n",
        "*** Begin Patch\n*** Add File: ok.txt\n+x\n*** Update File: a.txt\n*** Move to: ../moved.txt\n@@\n-a\n+b\n*** End Patch\n",
    ] {
        let refused = call_in(tool, &executor, &mut lease, json!({PATCH: patch})).await;

        assert_eq!(
            refused.error_code,
            Some(ToolErrorCode::PathNotAllowed),
            "{refused:?}"
        );
        assert!(
            refused.text.contains("leaves the workspace"),
            "{}",
            refused.text
        );
    }
}

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

/// `original` with the one update hunk of `patch` applied: the contents and
/// the lines added and removed, or the parser's own error.
fn applied(
    patch: &str,
    original: &str,
) -> Result<(String, usize, usize), super::codex::ApplyPatchError> {
    let hunks = super::codex::parse_patch(patch)?;
    let Some(super::codex::Hunk::UpdateFile { chunks, .. }) = hunks.first() else {
        return Err(super::codex::ApplyPatchError::ComputeReplacements(
            "the patch holds one update hunk".to_owned(),
        ));
    };
    let updated = super::codex::updated("f.txt", original, chunks)?;
    Ok((updated.contents, updated.added, updated.removed))
}

#[test]
fn an_update_with_context_lines_around_the_change_keeps_them_and_counts_only_the_change() {
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "@@\n",
        " a\n",
        "-b\n",
        "+B\n",
        " c\n",
        "*** End Patch\n",
    );

    assert_eq!(
        applied(patch, "a\nb\nc\nd\n"),
        Ok(("a\nB\nc\nd\n".to_owned(), 1, 1))
    );
}

/// Models end a region with an empty `-` line for the file's last newline;
/// the match is retried without it.
#[test]
fn an_update_whose_old_lines_end_in_an_empty_line_is_retried_without_it() {
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "@@\n",
        "-b\n",
        "-\n",
        "+B\n",
        "+\n",
        "*** End Patch\n",
    );

    assert_eq!(applied(patch, "a\nb\n"), Ok(("a\nB\n".to_owned(), 1, 1)));
}

#[test]
fn an_update_marked_end_of_file_seeks_from_the_end() {
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "@@\n",
        "-x\n",
        "+Z\n",
        "*** End of File\n",
        "*** End Patch\n",
    );

    assert_eq!(
        applied(patch, "x\ny\nx\n"),
        Ok(("x\ny\nZ\n".to_owned(), 1, 1))
    );
}

#[test]
fn a_context_marker_moves_each_chunk_past_the_one_before() {
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "@@\n",
        "-v\n",
        "+V1\n",
        "@@ k\n",
        "-v\n",
        "+V2\n",
        "*** End Patch\n",
    );

    assert_eq!(
        applied(patch, "k\nv\nk\nv\n"),
        Ok(("k\nV1\nk\nV2\n".to_owned(), 2, 2))
    );
}

#[test]
fn a_context_marker_that_is_not_in_the_file_is_refused_by_name() {
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "@@ nope\n",
        "-a\n",
        "+b\n",
        "*** End Patch\n",
    );

    assert_eq!(
        applied(patch, "a\n"),
        Err(super::codex::ApplyPatchError::ComputeReplacements(
            "Failed to find context 'nope' in f.txt".to_owned()
        ))
    );
}

#[test]
fn an_insertion_with_no_old_lines_lands_at_the_end_of_the_file() {
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "@@\n",
        "+tail\n",
        "*** End Patch\n",
    );

    assert_eq!(
        applied(patch, "a\nb\n"),
        Ok(("a\nb\ntail\n".to_owned(), 1, 0))
    );
}
