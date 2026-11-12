//! Usage the relay cannot read: skipped, left unsettled, and named once.
//!
//! To price and settle a group of lines the relay must recognise its work type,
//! each line's kind and its pricing posture. A group it cannot read is not an
//! error: one unreadable row must not hold the relay's cursor, and with it
//! every tenant's settlement behind it. It is not guessed at either, because a
//! guessed posture settles the wrong money. The group stays unsettled: the
//! dashboard shows it and `unsettled_oldest_seconds` keeps rising, while the
//! by-type `unsettled_nanos` figure totals only the two work types this build
//! names, so a group of an unknown work type is missing from that one. The first
//! sighting of each distinct spelling is logged, so an operator learns what is
//! stuck once rather than on every two-second pass.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, PoisonError};

/// A usage group the relay skipped.
const EVENT_SKIPPED: &str = "billing_relay_skipped";

/// How many distinct unreadable spellings are remembered.
///
/// The values come from this daemon's own writers, so a handful is the
/// realistic count; the cap keeps a hand-edited table from growing the set
/// without bound. Past it a new spelling is not logged: the earlier lines have
/// already named the problem, and the row is still counted as owed.
const REMEMBERED_MAX: usize = 64;

/// What the relay could not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Unreadable {
    /// A work type this build does not name.
    WorkType,
    /// A line kind this build does not name.
    LineKind,
    /// A pricing posture this build does not name.
    Posture,
}

impl Unreadable {
    const fn as_str(self) -> &'static str {
        match self {
            Self::WorkType => "work_type",
            Self::LineKind => "line_kind",
            Self::Posture => "posture",
        }
    }
}

/// The longest spelling remembered or logged, in bytes: a hand-edited value
/// cannot put an unbounded string into the set or the log.
const SPELLING_MAX: usize = 64;

/// Every spelling already logged this process: what it was read as, then the
/// work type it was met on. Nested so a spelling already named is found by
/// borrowed text, without building an owned key on every two-second pass.
struct Named {
    spellings: BTreeMap<Unreadable, BTreeMap<String, BTreeSet<String>>>,
    count: usize,
}

impl Named {
    const fn new() -> Self {
        Self {
            spellings: BTreeMap::new(),
            count: 0,
        }
    }
}

static NAMED: Mutex<Named> = Mutex::new(Named::new());

/// Whether this sighting is new, and room remains to remember it.
fn remember(named: &mut Named, what: Unreadable, work_type: &str, spelling: &str) -> bool {
    let known = named
        .spellings
        .get(&what)
        .and_then(|by_work_type| by_work_type.get(work_type))
        .is_some_and(|spellings| spellings.contains(spelling));
    if known || named.count >= REMEMBERED_MAX {
        return false;
    }
    named
        .spellings
        .entry(what)
        .or_default()
        .entry(work_type.to_owned())
        .or_default()
        .insert(spelling.to_owned());
    named.count += 1;
    true
}

/// `text` cut to at most [`SPELLING_MAX`] bytes, on a character boundary.
fn bounded(text: &str) -> &str {
    if text.len() <= SPELLING_MAX {
        return text;
    }
    let mut end = SPELLING_MAX;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or_default()
}

/// Logs `what`, spelled `spelling` on a line of `work_type`, the first time
/// this process meets that spelling.
pub fn skipped(what: Unreadable, work_type: &str, spelling: &str) {
    let (work_type, spelling) = (bounded(work_type), bounded(spelling));
    let first = {
        let mut named = NAMED.lock().unwrap_or_else(PoisonError::into_inner);
        remember(&mut named, what, work_type, spelling)
    };
    if first {
        // Bound first: the macro repeats its field expressions.
        let unreadable = what.as_str();
        tracing::warn!(
            unreadable,
            work_type,
            spelling,
            event = EVENT_SKIPPED,
            "the billing relay cannot read this usage; it stays unsettled and the relay moves on"
        );
    }
}

#[cfg(all(test, feature = "test-util"))]
#[expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]
mod tests {
    use af_core::test_util::trace::Capture;

    use super::{
        EVENT_SKIPPED, Named, REMEMBERED_MAX, SPELLING_MAX, Unreadable, remember, skipped,
    };

    fn logged(capture: &Capture) -> usize {
        capture
            .events()
            .iter()
            .filter(|event| event.field("event") == Some(EVENT_SKIPPED))
            .count()
    }

    /// A spelling is named the first time and not again, however many passes
    /// meet it.
    #[test]
    fn a_spelling_is_named_once_per_process() {
        let capture = Capture::install();
        for _pass in 0..5 {
            skipped(
                Unreadable::LineKind,
                "authoring_attempt",
                "once-per-process",
            );
        }
        assert_eq!(logged(&capture), 1);

        let named = capture
            .events()
            .into_iter()
            .find(|event| event.field("event") == Some(EVENT_SKIPPED))
            .expect("the first sighting was logged");
        assert_eq!(named.field("unreadable"), Some("line_kind"));
        assert_eq!(named.field("work_type"), Some("authoring_attempt"));
        assert_eq!(named.field("spelling"), Some("once-per-process"));
    }

    /// A hand-edited table cannot grow the set without bound: past the cap a new
    /// spelling is not remembered, and an old one is still recognised.
    #[test]
    fn the_set_stops_growing_at_its_cap() {
        let mut named = Named::new();
        for index in 0..REMEMBERED_MAX {
            assert!(remember(
                &mut named,
                Unreadable::LineKind,
                "fleet_lease",
                &index.to_string()
            ));
        }
        assert!(!remember(
            &mut named,
            Unreadable::LineKind,
            "fleet_lease",
            "one-more"
        ));
        assert_eq!(named.count, REMEMBERED_MAX);
        assert!(!remember(
            &mut named,
            Unreadable::LineKind,
            "fleet_lease",
            "0"
        ));
    }

    /// A hand-edited value is cut before it is stored or logged, on a
    /// character boundary.
    #[test]
    fn a_long_spelling_is_cut_before_it_is_logged() {
        let capture = Capture::install();
        skipped(Unreadable::LineKind, "fleet_lease", &"é".repeat(200));
        let named = capture
            .events()
            .into_iter()
            .find(|event| event.field("event") == Some(EVENT_SKIPPED))
            .expect("the spelling was logged");
        let spelling = named.field("spelling").expect("the spelling is a field");
        assert!(spelling.len() <= SPELLING_MAX, "{}", spelling.len());
        assert!(spelling.chars().all(|letter| letter == 'é'));
    }

    /// The same spelling meant differently, or on another work type, is a new
    /// sighting.
    #[test]
    fn a_different_meaning_or_work_type_is_named_again() {
        let capture = Capture::install();
        skipped(Unreadable::Posture, "fleet_lease", "distinct-spelling");
        skipped(
            Unreadable::Posture,
            "authoring_attempt",
            "distinct-spelling",
        );
        skipped(Unreadable::LineKind, "fleet_lease", "distinct-spelling");
        assert_eq!(logged(&capture), 3);
    }
}
