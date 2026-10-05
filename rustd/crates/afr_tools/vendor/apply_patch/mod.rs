//! Codex's `apply_patch` grammar and chunk matching, copied from
//! `openai/codex` at `2e5fea64eefcaa19f48458b2386011b619f69c70`
//! (`codex-rs/apply-patch/src/`), Apache-2.0; `NOTICE` beside this file lists
//! what was left out and what changed. The copy keeps Codex's names, shape
//! and style so a later pull from upstream diffs cleanly, which is why the
//! workspace's lints are allowed here rather than met.

#![allow(
    missing_debug_implementations,
    clippy::enum_variant_names,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::enum_glob_use,
    clippy::wildcard_imports,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::manual_let_else,
    clippy::if_not_else,
    clippy::needless_pass_by_value,
    clippy::doc_markdown,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::redundant_else,
    clippy::items_after_test_module,
    clippy::needless_continue,
    clippy::manual_strip,
    clippy::semicolon_if_nothing_returned,
    clippy::implicit_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::needless_raw_string_hashes,
    clippy::match_same_arms,
    clippy::items_after_statements,
    reason = "vendored from openai/codex at 2e5fea64e and kept as copied; see NOTICE"
)]

mod file_update;
mod parser;
mod seek_sequence;
mod streaming_parser;
mod text_file;

pub(crate) use self::file_update::updated;
pub(crate) use self::parser::{Hunk, ParseError, parse_patch};

/// Why a patch could not be applied: upstream's `ApplyPatchError`, less the
/// input/output arms, because the sandbox's executor reports those itself.
#[derive(Debug, thiserror::Error, PartialEq)]
pub(crate) enum ApplyPatchError {
    #[error(transparent)]
    ParseError(#[from] ParseError),
    /// Error that occurs while computing replacements when applying patch chunks
    #[error("{0}")]
    ComputeReplacements(String),
}
