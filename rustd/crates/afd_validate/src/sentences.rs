//! A route's sentences, keyed by the path a garde report names.
//!
//! A refusal's wording is a public commitment the dashboard renders, and
//! garde's own messages are not that wording. Four handlers each hand-wrote a
//! function from a report to a sentence; this is that function written once,
//! with the per-route part reduced to what it always was — a table.

/// One route's table from a reported path to the sentence that path earns.
///
/// Entries are tried in table order, so the table states which break wins
/// when one body breaks two bounds. A path the table does not name answers
/// the fallback, never garde's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sentences {
    entries: &'static [(&'static str, &'static str)],
    fallback: &'static str,
}

impl Sentences {
    /// A table of `(path, sentence)` entries and the sentence for any other path.
    ///
    /// A path is spelled as garde displays it: `provider`, `rates.max_tokens`,
    /// `binds[0].note`.
    #[must_use]
    pub const fn new(
        entries: &'static [(&'static str, &'static str)],
        fallback: &'static str,
    ) -> Self {
        Self { entries, fallback }
    }

    /// The sentence for the first entry whose path `report` names, else the
    /// fallback.
    #[must_use]
    pub fn pick(&self, report: &garde::Report) -> &'static str {
        // Each reported path is rendered once; a refusal is the cold path, and
        // a report carries a handful of entries at most.
        let reported: Vec<String> = report
            .iter()
            .map(|(path, _error)| path.to_string())
            .collect();
        self.entries
            .iter()
            .find(|(path, _sentence)| reported.iter().any(|seen| seen == path))
            .map_or(self.fallback, |(_path, sentence)| sentence)
    }
}

#[cfg(test)]
#[path = "sentences/tests.rs"]
mod tests;
