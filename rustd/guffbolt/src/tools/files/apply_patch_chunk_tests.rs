//! The chunk matching Codex's parser does, on text alone.

use std::path::PathBuf;

use super::codex::{ApplyPatchError, Hunk, ParseError, UpdateFileChunk, parse_patch, updated};

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

/// The one update hunk on `f.txt` that `chunks` make.
fn update(chunks: Vec<UpdateFileChunk>) -> Vec<Hunk> {
    vec![Hunk::UpdateFile {
        path: PathBuf::from("f.txt"),
        move_path: None,
        chunks,
    }]
}

/// A first changed line with no `@@` marker opens the hunk's first chunk.
#[test]
fn a_first_added_removed_or_blank_line_opens_a_chunk_without_a_marker() {
    let added = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "+new\n",
        "*** End Patch\n",
    );
    assert_eq!(
        parse_patch(added),
        Ok(update(vec![UpdateFileChunk {
            new_lines: vec!["new".to_owned()],
            ..UpdateFileChunk::default()
        }]))
    );

    let removed = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "-old\n",
        "*** End Patch\n",
    );
    assert_eq!(
        parse_patch(removed),
        Ok(update(vec![UpdateFileChunk {
            old_lines: vec!["old".to_owned()],
            ..UpdateFileChunk::default()
        }]))
    );

    let blank = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "\n",
        "+y\n",
        "*** End Patch\n",
    );
    assert_eq!(
        parse_patch(blank),
        Ok(update(vec![UpdateFileChunk {
            old_lines: vec![String::new()],
            new_lines: vec![String::new(), "y".to_owned()],
            context_line_indices: vec![(0, 0)],
            ..UpdateFileChunk::default()
        }]))
    );
}

#[test]
fn an_update_line_with_no_change_prefix_is_refused_with_its_line_number() {
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "bogus\n",
        "*** End Patch\n",
    );

    assert_eq!(
        parse_patch(patch),
        Err(ParseError::InvalidHunkError {
            message: "Unexpected line found in update hunk: 'bogus'. Every line should start with ' ' (context line), '+' (added line), or '-' (removed line)".to_owned(),
            line_number: 3,
        })
    );
}

#[test]
fn a_change_after_the_end_of_file_marker_needs_a_new_context_marker() {
    let unmarked = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "@@\n",
        "-x\n",
        "*** End of File\n",
        "+z\n",
        "*** End Patch\n",
    );
    assert_eq!(
        parse_patch(unmarked),
        Err(ParseError::InvalidHunkError {
            message: "Expected update hunk to start with a @@ context marker, got: '+z'".to_owned(),
            line_number: 6,
        })
    );

    let marked = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "@@\n",
        "-x\n",
        "+X\n",
        "*** End of File\n",
        "@@ k\n",
        "-y\n",
        "+Y\n",
        "*** End Patch\n",
    );
    assert_eq!(
        parse_patch(marked),
        Ok(update(vec![
            UpdateFileChunk {
                old_lines: vec!["x".to_owned()],
                new_lines: vec!["X".to_owned()],
                is_end_of_file: true,
                ..UpdateFileChunk::default()
            },
            UpdateFileChunk {
                change_context: Some("k".to_owned()),
                old_lines: vec!["y".to_owned()],
                new_lines: vec!["Y".to_owned()],
                ..UpdateFileChunk::default()
            },
        ]))
    );
}

/// Typographic dashes, quotes and spaces in the file match their ASCII
/// spelling in the patch, past a line that matches under no rule.
#[test]
fn typographic_punctuation_in_the_file_matches_its_ascii_spelling() {
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "@@\n",
        "-a-b'c\"d e\n",
        "+fixed\n",
        "*** End Patch\n",
    );

    assert_eq!(
        applied(patch, "zzz\na\u{2014}b\u{2019}c\u{201C}d\u{00A0}e\n"),
        Ok(("zzz\nfixed\n".to_owned(), 1, 1))
    );
}

/// A lone empty `-` line names only the final newline, so it matches at the
/// cursor and the added line is inserted there.
#[test]
fn an_update_removing_only_the_final_newline_inserts_at_the_cursor() {
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "@@\n",
        "-\n",
        "+x\n",
        "*** End Patch\n",
    );

    assert_eq!(applied(patch, "a\n"), Ok(("x\na\n".to_owned(), 1, 0)));
}

/// Carriage-return endings survive, an unterminated last line is matched,
/// and the replacement takes the file's own ending.
#[test]
fn a_carriage_return_file_with_an_unterminated_last_line_keeps_its_ending() {
    let patch = concat!(
        "*** Begin Patch\n",
        "*** Update File: f.txt\n",
        "@@\n",
        "-b\n",
        "+B\n",
        "*** End Patch\n",
    );

    assert_eq!(applied(patch, "a\rb"), Ok(("a\rB\r".to_owned(), 1, 1)));
}
