//! Applying an update hunk's chunks to a file's text: upstream
//! `file_update.rs`'s `compute_replacements` in its PreserveLineEndings mode,
//! with the file's text given rather than read, and the lines each
//! replacement adds and removes counted for the caller.

use super::ApplyPatchError;
use super::parser::UpdateFileChunk;
use super::seek_sequence;
use super::text_file::{Replacement, SourceFile};

/// A file after its chunks landed, and what changed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Updated {
    pub(crate) contents: String,
    pub(crate) added: usize,
    pub(crate) removed: usize,
}

/// `original` with `chunks` applied, each line keeping its own ending.
pub(crate) fn updated(
    path: &str,
    original: &str,
    chunks: &[UpdateFileChunk],
) -> Result<Updated, ApplyPatchError> {
    let mut source_file = SourceFile::parse(original);
    let original_lines = source_file.line_texts();
    let replacements = compute_replacements(&original_lines, path, chunks)?;
    let (added, removed) = replacements.iter().fold(
        (0, 0),
        |(added, removed), (_start_idx, old_len, new_segment)| {
            (added + new_segment.len(), removed + old_len)
        },
    );
    source_file.apply_replacements(&replacements);
    Ok(Updated {
        contents: source_file.into_contents(),
        added,
        removed,
    })
}

/// Compute a list of replacements needed to transform `original_lines` into the
/// new lines, given the patch `chunks`. Each replacement is returned as
/// `(start_index, old_len, new_lines)`.
fn compute_replacements(
    original_lines: &[String],
    path: &str,
    chunks: &[UpdateFileChunk],
) -> Result<Vec<Replacement>, ApplyPatchError> {
    let mut replacements: Vec<Replacement> = Vec::new();
    let mut line_index: usize = 0;

    for chunk in chunks {
        // If a chunk has a `change_context`, we use seek_sequence to find it, then
        // adjust our `line_index` to continue from there.
        if let Some(ctx_line) = &chunk.change_context {
            if let Some(idx) = seek_sequence::seek_sequence(
                original_lines,
                std::slice::from_ref(ctx_line),
                line_index,
                /*eof*/ false,
            ) {
                line_index = idx + 1;
            } else {
                return Err(ApplyPatchError::ComputeReplacements(format!(
                    "Failed to find context '{ctx_line}' in {path}"
                )));
            }
        }

        if chunk.old_lines.is_empty() {
            // `SourceFile` only exposes real source lines, so its insertion
            // point is always after the final line.
            let insertion_idx = original_lines.len();
            replacements.push((insertion_idx, 0, chunk.new_lines.clone()));
            continue;
        }

        // Otherwise, try to match the existing lines in the file with the old lines
        // from the chunk. If found, schedule that region for replacement.
        // Attempt to locate the `old_lines` verbatim within the file.  In many
        // real‑world diffs the last element of `old_lines` is an *empty* string
        // representing the terminating newline of the region being replaced.
        // This sentinel is not present in `original_lines` because `SourceFile`
        // stores the terminator on the preceding line rather than as an extra
        // trailing element. If a direct search fails and the pattern ends with
        // an empty string, retry without that final element so modifications
        // touching the end‑of‑file can be located reliably.

        let mut pattern: &[String] = &chunk.old_lines;
        let mut found =
            seek_sequence::seek_sequence(original_lines, pattern, line_index, chunk.is_end_of_file);

        let mut new_slice: &[String] = &chunk.new_lines;

        if found.is_none() && pattern.last().is_some_and(String::is_empty) {
            // Retry without the trailing empty line which represents the final
            // newline in the file.
            pattern = &pattern[..pattern.len() - 1];
            if new_slice.last().is_some_and(String::is_empty) {
                new_slice = &new_slice[..new_slice.len() - 1];
            }

            found = seek_sequence::seek_sequence(
                original_lines,
                pattern,
                line_index,
                chunk.is_end_of_file,
            );
        }

        if let Some(start_idx) = found {
            // Context lines occur in both sides of a patch chunk. Keep those
            // original lines in place so their exact contents and terminators
            // survive, especially when the file has mixed line endings.
            let mut old_start = 0;
            let mut new_start = 0;
            for &(old_context, new_context) in &chunk.context_line_indices {
                // A trailing empty context line can be removed from `pattern`
                // and `new_slice` above when it represents the final newline.
                if old_context >= pattern.len() || new_context >= new_slice.len() {
                    break;
                }
                if old_start != old_context || new_start != new_context {
                    replacements.push((
                        start_idx + old_start,
                        old_context - old_start,
                        new_slice[new_start..new_context].to_vec(),
                    ));
                }
                old_start = old_context + 1;
                new_start = new_context + 1;
            }
            if old_start != pattern.len() || new_start != new_slice.len() {
                replacements.push((
                    start_idx + old_start,
                    pattern.len() - old_start,
                    new_slice[new_start..].to_vec(),
                ));
            }
            line_index = start_idx + pattern.len();
        } else {
            return Err(ApplyPatchError::ComputeReplacements(format!(
                "Failed to find expected lines in {}:\n{}",
                path,
                chunk.old_lines.join("\n"),
            )));
        }
    }

    replacements.sort_by_key(|(index, _, _)| *index);

    Ok(replacements)
}
