//! The chunk matching Codex's parser does, on text alone.

use super::codex::{ApplyPatchError, Hunk, parse_patch, updated};

/// `original` with the one update hunk of `patch` applied: the contents and
/// the lines added and removed, or the parser's own error.
fn applied(patch: &str, original: &str) -> Result<(String, usize, usize), ApplyPatchError> {
    let hunks = parse_patch(patch)?;
    let Some(Hunk::UpdateFile { chunks, .. }) = hunks.first() else {
        return Err(ApplyPatchError::ComputeReplacements(
            "the patch holds one update hunk".to_owned(),
        ));
    };
    let updated = updated("f.txt", original, chunks)?;
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
        Err(ApplyPatchError::ComputeReplacements(
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
