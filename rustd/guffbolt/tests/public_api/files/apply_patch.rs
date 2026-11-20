//! `apply_patch` lands its hunks in order, as the files are.

use super::*;

#[tokio::test]
async fn apply_patch_adds_updates_moves_and_deletes_in_one_patch() {
    let desk = Desk::new();
    desk.put("keep.txt", "one\ntwo\nthree\n");
    desk.put("mv.txt", "m\n");
    desk.put("old.txt", "x\ny\n");
    let tools = desk.tools(false);
    let patch = "*** Begin Patch\n\
                 *** Add File: new.txt\n\
                 +hello\n\
                 +w\u{00f6}rld\n\
                 *** Update File: keep.txt\n\
                 @@\n \
                 one\n\
                 -two\n\
                 +2\n \
                 three\n\
                 *** Update File: mv.txt\n\
                 *** Move to: moved/mv.txt\n\
                 @@\n\
                 -m\n\
                 +n\n\
                 *** Delete File: old.txt\n\
                 *** End Patch";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(
        output.text,
        "Success. Updated the following files:\nA new.txt\nM keep.txt\nM moved/mv.txt\nD old.txt\n\
         Lines changed:\n\"new.txt\" +2 -0\n\"keep.txt\" +1 -1\n\"moved/mv.txt\" +1 -1\n\"old.txt\" +0 -2"
    );
    assert_eq!(
        desk.text("new.txt").as_deref(),
        Some("hello\nw\u{00f6}rld\n")
    );
    assert_eq!(desk.text("keep.txt").as_deref(), Some("one\n2\nthree\n"));
    assert_eq!(desk.text("moved/mv.txt").as_deref(), Some("n\n"));
    assert_eq!(desk.text("mv.txt"), None);
    assert_eq!(desk.text("old.txt"), None);
}

/// Hunks on one file apply in order, each to what the one before left, and
/// a file one hunk touched twice is counted once.
#[tokio::test]
async fn apply_patch_applies_later_hunks_to_what_earlier_ones_left() {
    let desk = Desk::new();
    desk.put("f.txt", "a\nb\n");
    let tools = desk.tools(false);
    let patch = "*** Begin Patch\n\
                 *** Update File: f.txt\n\
                 -a\n\
                 +A\n\
                 *** Update File: ./f.txt\n\
                 *** Move to: f.txt\n\
                 @@ A\n\
                 -b\n\
                 +B\n\
                 *** End Patch";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(
        output.text,
        "Success. Updated the following files:\nM f.txt\nM ./f.txt\nLines changed:\n\"f.txt\" +1 -1\n\"./f.txt\" +1 -1"
    );
    assert_eq!(desk.text("f.txt").as_deref(), Some("A\nB\n"));
}

/// Codex reads a patch wrapped in a heredoc, which one model writes, and a
/// file's own line endings survive an update.
#[tokio::test]
async fn apply_patch_reads_a_heredoc_and_keeps_a_files_line_endings() {
    let desk = Desk::new();
    desk.put("crlf.txt", "a\r\nb\r\n");
    desk.put("cr.txt", "a\rb\r");
    let tools = desk.tools(false);
    let patch = "<<'EOF'\n\
                 *** Begin Patch\n\
                 *** Update File: crlf.txt\n\
                 @@\n \
                 a\n\
                 -b\n\
                 +B\n\
                 +C\n\
                 *** Update File: cr.txt\n\
                 @@\n\
                 +c\n\
                 *** End Patch\n\
                 \n\
                 *** End Patch\n\
                 EOF";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(
        output.text,
        "Success. Updated the following files:\nM crlf.txt\nM cr.txt\nLines changed:\n\"crlf.txt\" +2 -1\n\"cr.txt\" +1 -0"
    );
    assert_eq!(desk.text("crlf.txt").as_deref(), Some("a\r\nB\r\nC\r\n"));
    assert_eq!(desk.text("cr.txt").as_deref(), Some("a\rb\rc\r"));
}

/// An update hunk may start its lines without an `@@` marker.
#[tokio::test]
async fn apply_patch_takes_an_update_whose_lines_need_no_marker() {
    let desk = Desk::new();
    desk.put("ctx.txt", "a\nb\n");
    desk.put("minus.txt", "a\nb\n");
    desk.put("plus.txt", "a\n");
    desk.put("blank.txt", "\nb\n");
    let tools = desk.tools(false);
    let patch = "*** Begin Patch\n\
                 *** Update File: ctx.txt\n \
                 a\n\
                 -b\n\
                 *** Update File: minus.txt\n\
                 -a\n\
                 *** Update File: plus.txt\n\
                 +b\n\
                 *** Update File: blank.txt\n\
                 \n\
                 -b\n\
                 +B\n\
                 *** End Patch";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(code(&output), None, "{}", output.text);
    assert_eq!(desk.text("ctx.txt").as_deref(), Some("a\n"));
    assert_eq!(desk.text("minus.txt").as_deref(), Some("b\n"));
    assert_eq!(desk.text("plus.txt").as_deref(), Some("a\nb\n"));
    assert_eq!(desk.text("blank.txt").as_deref(), Some("\nB\n"));
}

/// Two spellings of one file, a link and its target, are planned apart and
/// land one after the other, each applied to the file as it is then.
#[cfg(unix)]
#[tokio::test]
async fn apply_patch_through_a_link_lands_each_hunk_on_the_file_as_it_is() {
    let desk = Desk::new();
    desk.put("real.txt", "a\nb\n");
    std::os::unix::fs::symlink(desk.path("real.txt"), desk.path("alias.txt")).expect("linked");
    let tools = desk.tools(false);
    let patch = "*** Begin Patch\n\
                 *** Update File: real.txt\n\
                 -a\n\
                 +A\n\
                 *** Update File: alias.txt\n\
                 -b\n\
                 +B\n\
                 *** End Patch";
    let stale = "*** Begin Patch\n\
                 *** Update File: real.txt\n\
                 -A\n\
                 +a\n\
                 *** Update File: alias.txt\n\
                 -A\n\
                 +x\n\
                 *** End Patch";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;
    let landed = desk.text("real.txt");
    let half = call(&tools, "apply_patch", json!({"patch": stale})).await;

    assert_eq!(code(&output), None, "{}", output.text);
    assert_eq!(landed.as_deref(), Some("A\nB\n"));
    assert_eq!(
        half.text, "[patch_invalid] Failed to find expected lines in alias.txt:\nA",
        "the second hunk no longer applies once the first landed"
    );
    assert_eq!(
        desk.text("real.txt").as_deref(),
        Some("a\nB\n"),
        "the hunk before it stays landed"
    );
}

/// A write the environment refuses as the patch lands is reported, and the
/// hunks before it stay landed.
#[tokio::test]
async fn apply_patch_reports_a_write_refused_as_it_lands_and_keeps_what_landed() {
    let desk = Desk::new();
    std::fs::create_dir(desk.path("taken")).expect("made");
    let tools = desk.tools(false);
    let patch = wrapped("*** Add File: first.txt\n+one\n*** Add File: taken\n+two");

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(
        code(&output),
        Some("sandbox_unavailable"),
        "{}",
        output.text
    );
    assert_eq!(desk.text("first.txt").as_deref(), Some("one\n"));
    assert!(desk.path("taken").is_dir(), "the directory stays");
}
