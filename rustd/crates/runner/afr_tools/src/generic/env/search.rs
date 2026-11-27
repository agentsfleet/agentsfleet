//! A tool's search as the guest agent's `fs/search` reads it, and the
//! guest agent's answer as the tool reads it: the same fields, one to one.

use guffbolt::tools::env::{Found, Hit, Pattern, Search};

/// `search` as the guest agent reads it.
pub(super) fn search_of(search: &Search) -> af_guest_wire::Search {
    af_guest_wire::Search {
        path: search.path.clone(),
        glob: search.glob.clone(),
        pattern: search.pattern.as_ref().map(pattern_of),
        limit: u64::try_from(search.limit).unwrap_or(u64::MAX),
    }
}

fn pattern_of(pattern: &Pattern) -> af_guest_wire::Pattern {
    af_guest_wire::Pattern {
        text: pattern.text.clone(),
        literal: pattern.literal,
        ignore_case: pattern.ignore_case,
    }
}

/// The guest agent's answer as a tool reads it.
pub(super) fn found_of(found: af_guest_wire::Found) -> Found {
    Found {
        hits: found.hits.into_iter().map(hit_of).collect(),
        limited: found.limited,
        cut: found.cut,
    }
}

fn hit_of(hit: af_guest_wire::Hit) -> Hit {
    match hit {
        af_guest_wire::Hit::File(path) => Hit::File(path),
        af_guest_wire::Hit::Line { path, number, text } => Hit::Line { path, number, text },
    }
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
