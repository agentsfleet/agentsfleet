//! A route's answers, keyed by the path a garde report names.
//!
//! A refusal's wording is a public commitment the dashboard renders, and
//! garde's own messages are not that wording. Four handlers each hand-wrote a
//! function from a report to a sentence, and the crates that answer with an
//! error variant instead of a sentence hand-wrote the same walk again. This is
//! that function written once; the per-route part is what it always was, a
//! table.

/// One route's table from a reported path to the answer that path earns.
///
/// Entries are tried in table order, so the table states which break wins
/// when one value breaks two bounds. A path the table does not name answers
/// the fallback, never garde's text.
///
/// A path is spelled as garde displays it, with every list index written
/// `[]`: `provider`, `rates.context_cap_tokens`, `extra_binds[].note`. An
/// index is where the break was, not which bound broke, so an entry matches
/// that field in every element. A struct-level `custom` rule reports at the
/// empty path, `""`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathTable<T: 'static> {
    entries: &'static [(&'static str, T)],
    fallback: T,
}

/// A route's table from a reported path to the sentence a caller reads.
pub type Sentences = PathTable<&'static str>;

impl<T: Copy> PathTable<T> {
    /// A table of `(path, answer)` entries and the answer for any other path.
    #[must_use]
    pub const fn new(entries: &'static [(&'static str, T)], fallback: T) -> Self {
        Self { entries, fallback }
    }

    /// The answer for the first entry whose path `report` names, else the
    /// fallback.
    #[must_use]
    pub fn pick(&self, report: &garde::Report) -> T {
        // Each reported path is rendered once; a refusal is the cold path, and
        // a report carries a handful of entries at most.
        let reported: Vec<String> = report
            .iter()
            .map(|(path, _error)| without_indices(&path.to_string()))
            .collect();
        self.entries
            .iter()
            .find(|(path, _answer)| reported.iter().any(|seen| seen == path))
            .map_or(self.fallback, |&(_path, answer)| answer)
    }
}

/// `binds[3].note` as `binds[].note`: the digits between brackets dropped.
fn without_indices(path: &str) -> String {
    let mut inside = false;
    path.chars()
        .filter(|&character| {
            match character {
                '[' => inside = true,
                ']' => inside = false,
                _ if inside => return false,
                _ => {}
            }
            true
        })
        .collect()
}

#[cfg(test)]
#[path = "sentences/tests.rs"]
mod tests;
