//! One pass's appends over its claimed rows, with no datastore in sight.
//!
//! Split from the statements around it so the ordering rules — append in
//! claim order, stop at the first refusal, keep every row the queue never
//! reached — are proved by a unit test with a fake queue.

use afd_dragonfly::EventId;

use super::Pending;

/// What a pass's appends came to.
#[derive(Debug)]
pub(super) struct Drained<'a> {
    /// The rows the queue took, in claim order, each with its receipt.
    pub(super) appended: Vec<(&'a Pending, EventId)>,
    /// The rows the pass never got to append: the one the queue refused and
    /// every row claimed after it.
    pub(super) unreached: &'a [Pending],
}

/// Appends `rows` in order through `append`, stopping at the first row it
/// answers `None` for.
///
/// `append` is called at most once per row and never after a refusal, so a
/// queue that is down costs one failed round trip per pass, not one per row.
pub(super) async fn drain<'a, F, Fut>(rows: &'a [Pending], mut append: F) -> Drained<'a>
where
    F: FnMut(&'a Pending) -> Fut,
    Fut: Future<Output = Option<EventId>>,
{
    let mut appended = Vec::with_capacity(rows.len());
    for (index, pending) in rows.iter().enumerate() {
        match append(pending).await {
            Some(receipt) => appended.push((pending, receipt)),
            None => {
                return Drained {
                    appended,
                    unreached: rows.get(index..).unwrap_or_default(),
                };
            }
        }
    }
    Drained {
        appended,
        unreached: &[],
    }
}

#[cfg(test)]
mod tests;
