//! Half-open spans that must not overlap under one key: a rule garde lacks,
//! for a list whose items each carry a start and an optional end.
//!
//! A dated price schedule is the caller this was written for: each prompt
//! tier is a key, and two quotes for one tier must never both apply at the
//! same instant.

/// What [`disjoint`] reports for a span that ends at or before its start.
pub const REVERSED: &str = "a span must end after it starts";
/// What [`disjoint`] reports for two spans under one key sharing an instant.
pub const OVERLAPPING: &str = "spans under one key must not overlap";

/// One half-open span `[start, end)` under `key`; an absent `end` is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span<K, T> {
    /// What the span is disjoint within.
    pub key: K,
    /// The first instant the span covers.
    pub start: T,
    /// The first instant it no longer covers, or `None` for an open span.
    pub end: Option<T>,
}

/// Refuses a span that ends at or before its start, and two spans under one
/// key that share any instant.
///
/// # Errors
/// [`REVERSED`] or [`OVERLAPPING`].
pub fn disjoint<K: Ord + Copy, T: Ord + Copy>(
    spans: impl IntoIterator<Item = Span<K, T>>,
) -> garde::Result {
    let mut sorted: Vec<Span<K, T>> = spans.into_iter().collect();
    if sorted
        .iter()
        .any(|span| span.end.is_some_and(|end| end <= span.start))
    {
        return Err(garde::Error::new(REVERSED));
    }
    sorted.sort_by_key(|span| (span.key, span.start));
    let overlap = sorted.windows(2).any(|pair| match pair {
        [earlier, later] => {
            earlier.key == later.key && earlier.end.is_none_or(|end| end > later.start)
        }
        _ => false,
    });
    if overlap {
        Err(garde::Error::new(OVERLAPPING))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
