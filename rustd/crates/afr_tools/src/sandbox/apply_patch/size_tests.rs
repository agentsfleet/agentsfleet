#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

//! A patch never writes a file it cannot read back whole. Landing re-reads an
//! update through `whole`, which refuses a file longer than the executor's
//! read cap, so a hunk that would grow a file past the cap is refused as it is
//! planned, before anything lands, and the file is left as it was. Planning
//! holds each spelling of a file apart, so hunks through a link and through
//! its target are held to the cap again as each lands.

use std::os::unix::fs::symlink;

use afr_executor::MAX_READ_BYTES;

use super::tests::{KEEP, apply, planted};
use crate::runtime::ToolErrorCode;

/// The file the hunks grow, and the two lines they anchor on.
const BIG: &str = "src/big.txt";
const HEAD: &str = "head\n";
const TAIL: &str = "tail\n";
/// A second name for [`KEEP`]: a link beside it, pointing at it.
const ALIAS: &str = "src/alias.txt";
/// The reproduction's insert: forty bytes, as one `+` line.
const INSERT_BYTES: usize = 40;
/// How far under the cap the reproduction's file sat before the insert.
const UNDER_CAP: usize = 20;

/// The cap, as a length.
fn cap() -> usize {
    usize::try_from(MAX_READ_BYTES).unwrap()
}

/// `HEAD`, one filler line and `TAIL`, `total` bytes in all.
fn body(total: usize) -> String {
    let filler = total - HEAD.len() - TAIL.len() - 1;
    format!("{HEAD}{}\n{TAIL}", "x".repeat(filler))
}

/// A hunk inserting `bytes` bytes, newline included, after `HEAD` in `path`.
fn grow(path: &str, bytes: usize) -> String {
    format!(
        "*** Update File: {path}\n@@\n {HEAD}+{}\n",
        "y".repeat(bytes - 1)
    )
}

/// A hunk turning `path`'s `tail` into `TAIL`.
fn retail(path: &str) -> String {
    format!("*** Update File: {path}\n@@\n-tail\n+TAIL\n")
}

/// `hunks`, as one patch.
fn patch(hunks: &[String]) -> String {
    format!("*** Begin Patch\n{}*** End Patch\n", hunks.concat())
}

/// The reproduction from the review of the landing order: a file twenty
/// bytes under the cap, a forty-byte insert, then a second hunk on the same
/// file. Landing would write the first and refuse to re-read for the second;
/// planning refuses the whole patch instead, naming the file, and the file
/// keeps every byte.
#[tokio::test]
async fn test_chained_patch_past_read_cap_is_refused_whole() {
    let live = planted().await;
    let before = body(cap() - UNDER_CAP);
    std::fs::write(live.root.join(BIG), &before).unwrap();

    let applied = apply(&live, &patch(&[grow(BIG, INSERT_BYTES), retail(BIG)])).await;

    assert_eq!(
        applied.error_code,
        Some(ToolErrorCode::FileTooLarge),
        "{applied:?}"
    );
    assert!(applied.text.contains(BIG), "{}", applied.text);
    assert_eq!(
        std::fs::read(live.root.join(BIG)).unwrap(),
        before.as_bytes(),
        "nothing landed"
    );
    live.stop().await;
}

/// One hunk past the cap is refused on its own, and the file is unchanged.
#[tokio::test]
async fn test_update_past_read_cap_is_refused() {
    let live = planted().await;
    let before = body(cap() - UNDER_CAP);
    std::fs::write(live.root.join(BIG), &before).unwrap();

    let applied = apply(&live, &patch(&[grow(BIG, INSERT_BYTES)])).await;

    assert_eq!(
        applied.error_code,
        Some(ToolErrorCode::FileTooLarge),
        "{applied:?}"
    );
    assert_eq!(
        std::fs::read_to_string(live.root.join(BIG)).unwrap(),
        before,
        "the file is as it was"
    );
    live.stop().await;
}

/// A result of exactly the cap is readable whole, so it lands, and a later
/// hunk on it in the same patch lands too: the bound is the cap, not under it.
#[tokio::test]
async fn test_update_to_exactly_read_cap_lands() {
    let live = planted().await;
    std::fs::write(live.root.join(BIG), body(cap() - INSERT_BYTES)).unwrap();

    let applied = apply(&live, &patch(&[grow(BIG, INSERT_BYTES), retail(BIG)])).await;

    assert_eq!(applied.error_code, None, "{applied:?}");
    let after = std::fs::read_to_string(live.root.join(BIG)).unwrap();
    assert_eq!(after.len(), cap(), "the file is exactly one read long");
    assert!(after.starts_with(HEAD), "the head stayed");
    assert!(after.ends_with("TAIL\n"), "the second hunk landed too");
    live.stop().await;
}

/// Through a link: the first hunk grows the target past the cap, the second
/// edits it by the link's name. Refused as planned; neither spelling of the
/// file changed, and the link is still a link.
#[tokio::test]
async fn test_link_alias_past_read_cap_is_refused() {
    let live = planted().await;
    let before = body(cap() - UNDER_CAP);
    std::fs::write(live.root.join(KEEP), &before).unwrap();
    symlink("keep.txt", live.root.join(ALIAS)).unwrap();

    let applied = apply(&live, &patch(&[grow(KEEP, INSERT_BYTES), retail(ALIAS)])).await;

    assert_eq!(
        applied.error_code,
        Some(ToolErrorCode::FileTooLarge),
        "{applied:?}"
    );
    assert!(applied.text.contains(KEEP), "{}", applied.text);
    assert_eq!(
        std::fs::read_to_string(live.root.join(KEEP)).unwrap(),
        before,
        "nothing landed through either name"
    );
    assert!(
        live.root
            .join(ALIAS)
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink(),
        "the link was not replaced"
    );
    live.stop().await;
}

/// Through a link, again: each hunk grows the file by less than the room left,
/// so each plans under the cap, but together they cross it. The first lands;
/// the second, re-read by the link's name, is refused as it lands, and the file
/// is never written past one read.
#[tokio::test]
async fn test_link_alias_growths_past_read_cap_stop_at_landing() {
    let live = planted().await;
    let start = cap() - UNDER_CAP - UNDER_CAP / 2;
    std::fs::write(live.root.join(KEEP), body(start)).unwrap();
    symlink("keep.txt", live.root.join(ALIAS)).unwrap();

    let applied = apply(
        &live,
        &patch(&[grow(KEEP, UNDER_CAP), grow(ALIAS, UNDER_CAP)]),
    )
    .await;

    assert_eq!(
        applied.error_code,
        Some(ToolErrorCode::FileTooLarge),
        "{applied:?}"
    );
    assert!(applied.text.contains(ALIAS), "{}", applied.text);
    let after = std::fs::read(live.root.join(KEEP)).unwrap();
    assert_eq!(
        after.len(),
        start + UNDER_CAP,
        "the first hunk landed, the second did not"
    );
    live.stop().await;
}
