//! A tool's search as the executor's `fs/search` reads it, and the
//! executor's answer as the tool reads it: the same fields, one to one.

use guffbolt::tools::env::{Found, Hit, Pattern, Search};

/// `search` as the executor reads it.
pub(super) fn search_of(search: &Search) -> afr_executor::Search {
    afr_executor::Search {
        path: search.path.clone(),
        glob: search.glob.clone(),
        pattern: search.pattern.as_ref().map(pattern_of),
        limit: u64::try_from(search.limit).unwrap_or(u64::MAX),
    }
}

fn pattern_of(pattern: &Pattern) -> afr_executor::Pattern {
    afr_executor::Pattern {
        text: pattern.text.clone(),
        literal: pattern.literal,
        ignore_case: pattern.ignore_case,
        context: u64::try_from(pattern.context).unwrap_or(u64::MAX),
        line_chars: pattern
            .line_chars
            .map(|n| u64::try_from(n).unwrap_or(u64::MAX)),
    }
}

/// The executor's answer as a tool reads it.
pub(super) fn found_of(found: afr_executor::Found) -> Found {
    Found {
        hits: found.hits.into_iter().map(hit_of).collect(),
        limited: found.limited,
        cut: found.cut,
    }
}

fn hit_of(hit: afr_executor::Hit) -> Hit {
    match hit {
        afr_executor::Hit::File(path) => Hit::File(path),
        afr_executor::Hit::Line { path, number, text } => Hit::Line { path, number, text },
    }
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
