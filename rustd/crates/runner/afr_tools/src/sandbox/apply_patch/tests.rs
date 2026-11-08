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
pub(super) const KEEP: &str = "src/keep.txt";
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
pub(super) async fn apply(live: &Live, patch: &str) -> ToolOutput {
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[APPLY_PATCH.name()]).unwrap();
    let lease = Lease::default();
    call_in(
        offered(&selection, &APPLY_PATCH),
        &live.client,
        &lease,
        json!({PATCH: patch}),
    )
    .await
}

/// A workspace holding [`KEEP`] and [`OLD`].
pub(super) async fn planted() -> Live {
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
    let lease = Lease::default();

    for patch in [
        "*** Begin Patch\n*** Add File: ../escape.txt\n+x\n*** End Patch\n",
        "*** Begin Patch\n*** Add File: /etc/planted\n+x\n*** End Patch\n",
        "*** Begin Patch\n*** Delete File: a/../../b\n*** End Patch\n",
        "*** Begin Patch\n*** Add File: ok.txt\n+x\n*** Update File: a.txt\n*** Move to: ../moved.txt\n@@\n-a\n+b\n*** End Patch\n",
    ] {
        let refused = call_in(tool, &executor, &lease, json!({PATCH: patch})).await;

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
