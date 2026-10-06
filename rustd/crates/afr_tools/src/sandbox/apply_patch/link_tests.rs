#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

//! A link inside the workspace gives one file a second name. The executor
//! reads and writes through it, so hunks naming the file each way must land on
//! the one file: each update reads its file again as it lands.

use std::os::unix::fs::symlink;

use super::tests::{KEEP, apply, planted};
use crate::runtime::ToolErrorCode;

/// The second name for [`KEEP`]: a link beside it, pointing at it.
const ALIAS: &str = "src/alias.txt";

/// A hunk through the link and a hunk through the target both land, and the
/// link is still a link: the second edit does not overwrite the first with
/// text planned from the file as it was.
#[tokio::test]
async fn should_keep_both_edits_when_a_patch_names_one_file_through_a_link_and_its_target() {
    let live = planted().await;
    symlink("keep.txt", live.root.join(ALIAS)).unwrap();
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: src/keep.txt\n",
        "@@\n",
        "-one\n",
        "+ONE\n",
        "*** Update File: src/alias.txt\n",
        "@@\n",
        "-three\n",
        "+THREE\n",
        "*** End Patch\n",
    );

    let applied = apply(&live, patch).await;

    assert_eq!(applied.error_code, None, "{applied:?}");
    assert_eq!(
        std::fs::read_to_string(live.root.join(KEEP)).unwrap(),
        "ONE\ntwo\nTHREE\n",
        "both edits are in the one file"
    );
    assert!(
        live.root
            .join(ALIAS)
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink(),
        "the link was written through, not replaced"
    );
    assert!(applied.text.ends_with("+2 \u{2212}2"), "{}", applied.text);
    live.stop().await;
}

/// Two names for one file can disagree: once the hunk through the target has
/// changed `two`, the hunk through the link no longer finds it. That hunk is
/// refused as it lands, and the edit before it stays, as a refused write's
/// does; the second hunk does not silently undo the first.
#[tokio::test]
async fn should_refuse_at_landing_when_the_hunk_through_the_link_no_longer_applies() {
    let live = planted().await;
    symlink("keep.txt", live.root.join(ALIAS)).unwrap();
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: src/keep.txt\n",
        "@@\n",
        "-two\n",
        "+TWO\n",
        "*** Update File: src/alias.txt\n",
        "@@\n",
        "-two\n",
        "+2\n",
        "*** End Patch\n",
    );

    let refused = apply(&live, patch).await;

    assert_eq!(
        refused.error_code,
        Some(ToolErrorCode::PatchInvalid),
        "{refused:?}"
    );
    assert!(refused.text.contains(ALIAS), "{}", refused.text);
    assert_eq!(
        std::fs::read_to_string(live.root.join(KEEP)).unwrap(),
        "one\nTWO\nthree\n",
        "the first hunk landed and the second changed nothing"
    );
    live.stop().await;
}

/// A hunk through the link after one that deleted its target finds no file
/// when it lands: it reads `file_not_found`, the delete before it stays, and
/// no file is made behind the dangling link.
#[tokio::test]
async fn should_read_file_not_found_at_landing_when_an_earlier_hunk_deleted_the_link_target() {
    let live = planted().await;
    symlink("keep.txt", live.root.join(ALIAS)).unwrap();
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Delete File: src/keep.txt\n",
        "*** Update File: src/alias.txt\n",
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
    assert!(
        !live.root.join(KEEP).exists(),
        "the delete landed, and nothing wrote the target back"
    );
    live.stop().await;
}
