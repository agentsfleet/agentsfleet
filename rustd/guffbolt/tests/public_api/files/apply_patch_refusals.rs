//! `apply_patch` refuses a patch it cannot land and changes nothing.

use super::*;

/// A patch that does not parse in Codex's grammar is refused in its words,
/// and changes no file.
#[tokio::test]
async fn apply_patch_refuses_a_patch_that_does_not_parse() {
    let desk = Desk::new();
    desk.put("keep.txt", "one\n");
    let tools = desk.tools(false);

    let no_begin = call(
        &tools,
        "apply_patch",
        json!({"patch": "hello\n*** End Patch"}),
    )
    .await;
    let no_end = call(
        &tools,
        "apply_patch",
        json!({"patch": "*** Begin Patch\n*** Add File: a\n+x"}),
    )
    .await;
    let one_line = call(&tools, "apply_patch", json!({"patch": "garbage"})).await;
    let empty = call(&tools, "apply_patch", json!({"patch": ""})).await;
    let no_hunks = call(
        &tools,
        "apply_patch",
        json!({"patch": "*** Begin Patch\n*** End Patch"}),
    )
    .await;
    let blank_line = call(&tools, "apply_patch", json!({"patch": wrapped("")})).await;
    let bad_line = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Update File: keep.txt\n@@\n?bad")}),
    )
    .await;
    let no_marker = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Update File: keep.txt\n-one\n+1\nstray")}),
    )
    .await;

    assert_eq!(
        no_begin.text,
        "[patch_invalid] invalid patch: The first line of the patch must be '*** Begin Patch'"
    );
    assert_eq!(
        no_end.text,
        "[patch_invalid] invalid patch: The last line of the patch must be '*** End Patch'"
    );
    assert_eq!(
        one_line.text,
        "[patch_invalid] invalid patch: The first line of the patch must be '*** Begin Patch'"
    );
    assert_eq!(
        empty.text,
        "[patch_invalid] invalid patch: The last line of the patch must be '*** End Patch'"
    );
    assert_eq!(no_hunks.text, "[patch_invalid] the patch changes no file");
    assert_eq!(
        blank_line.text,
        "[patch_invalid] invalid hunk at line 2, '' is not a valid hunk header. Valid hunk \
         headers: '*** Add File: {path}', '*** Delete File: {path}', '*** Update File: {path}'"
    );
    assert_eq!(
        bad_line.text,
        "[patch_invalid] invalid hunk at line 4, Unexpected line found in update hunk: '?bad'. \
         Every line should start with ' ' (context line), '+' (added line), or '-' (removed line)"
    );
    assert_eq!(
        no_marker.text,
        "[patch_invalid] invalid hunk at line 5, Expected update hunk to start with a @@ context \
         marker, got: 'stray'"
    );
    assert_eq!(desk.text("keep.txt").as_deref(), Some("one\n"));
    assert_eq!(desk.text("a"), None);
}

/// A patch whose hunk does not apply, or names a path it may not touch,
/// changes no file, even one an earlier hunk would have added.
#[tokio::test]
async fn apply_patch_refuses_a_hunk_that_does_not_apply_and_changes_nothing() {
    let desk = Desk::new();
    desk.put("keep.txt", "one\n");
    desk.put("old.txt", "x\n");
    let tools = desk.tools(false);

    let unmatched = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Add File: added.txt\n+new\n*** Update File: keep.txt\n-zzz\n+y")}),
    )
    .await;
    let after_delete = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Delete File: old.txt\n*** Update File: old.txt\n-x\n+y")}),
    )
    .await;
    let after_move = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Update File: old.txt\n*** Move to: new.txt\n-x\n+y\n*** Delete File: old.txt")}),
    )
    .await;
    let missing = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Delete File: nowhere.txt")}),
    )
    .await;
    let outside = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Add File: ../x.txt\n+x")}),
    )
    .await;
    let move_outside = call(
        &tools,
        "apply_patch",
        json!({"patch": wrapped("*** Update File: keep.txt\n*** Move to: /tmp/x.txt\n-one\n+1")}),
    )
    .await;

    assert_eq!(
        unmatched.text,
        "[patch_invalid] Failed to find expected lines in keep.txt:\nzzz"
    );
    assert_eq!(
        after_delete.text,
        "[file_not_found] old.txt was moved or deleted earlier in this patch"
    );
    assert_eq!(
        after_move.text,
        "[file_not_found] old.txt was moved or deleted earlier in this patch"
    );
    assert_eq!(code(&missing), Some("file_not_found"));
    assert_eq!(
        outside.text,
        format!("[path_not_allowed] ../x.txt {LEAVES}")
    );
    assert_eq!(
        move_outside.text,
        format!("[path_not_allowed] /tmp/x.txt {LEAVES}")
    );
    assert_eq!(
        desk.text("added.txt"),
        None,
        "the add before the bad hunk never landed"
    );
    assert_eq!(desk.text("keep.txt").as_deref(), Some("one\n"));
    assert_eq!(desk.text("old.txt").as_deref(), Some("x\n"));
    assert_eq!(desk.text("new.txt"), None);
}

/// A hunk whose result would be longer than one read carries is refused as
/// it is planned, and nothing lands.
#[tokio::test]
async fn apply_patch_refuses_a_result_longer_than_one_read() {
    let desk = Desk::new();
    desk.put("near.txt", format!("{}\n", "z".repeat(ONE_READ - 2)));
    let tools = desk.tools(false);
    let patch = "*** Begin Patch\n*** Update File: near.txt\n+more\n*** End Patch";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(
        output.text,
        "[file_too_large] near.txt is longer than 8388608 bytes"
    );
    assert_eq!(
        desk.text("near.txt").map(|text| text.len()),
        Some(ONE_READ - 1)
    );
}

/// An update to a file that cannot be read whole as text is refused.
#[tokio::test]
async fn apply_patch_refuses_an_update_to_a_file_that_is_not_text() {
    let desk = Desk::new();
    desk.put("bin.dat", [0xff_u8, b'\n']);
    let tools = desk.tools(false);
    let patch = "*** Begin Patch\n*** Update File: bin.dat\n+x\n*** End Patch";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(
        output.text,
        "[invalid_arguments] bin.dat is not text, so it cannot be edited"
    );
}
