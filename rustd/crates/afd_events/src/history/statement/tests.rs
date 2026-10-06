//! What the read texts promise without a database: one column list, bodies
//! only where they are paid for, placeholders numbered as `History` binds
//! them, and no predicate behind a NULL gate.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::collections::BTreeSet;

use super::*;

/// The texts outside the listing grid, with how many values `History` binds
/// to each: the thread's two, and one actor's two (`statement/actor.rs`).
const THREADS: [(&str, usize); 4] = [
    (SELECT_THREAD_PAGE, 3),
    (SELECT_THREAD_PAGE_AFTER, 5),
    (SELECT_FLEET_PAGE_OF_ACTOR, 4),
    (SELECT_FLEET_PAGE_OF_ACTOR_AFTER, 6),
];

/// Every `(fleet_scoped, resumes, by_actor)` a listing can be asked for.
fn listing_shapes() -> impl Iterator<Item = (bool, bool, bool)> {
    [false, true].into_iter().flat_map(|fleet| {
        [false, true]
            .into_iter()
            .flat_map(move |resumes| [false, true].map(|actor| (fleet, resumes, actor)))
    })
}

/// How many values `History::page` binds for a listing shape: the workspace,
/// `since` and the limit always, then the fleet, the cursor pair and the
/// actor pattern when present.
fn listing_binds((fleet, resumes, actor): (bool, bool, bool)) -> usize {
    3 + usize::from(fleet) + 2 * usize::from(resumes) + usize::from(actor)
}

/// Every listing and thread text, with how many values each is bound.
fn every_text() -> Vec<(&'static str, usize)> {
    listing_shapes()
        .map(|shape| {
            (
                listing_text(shape.0, shape.1, shape.2),
                listing_binds(shape),
            )
        })
        .chain(THREADS)
        .collect()
}

/// The numbers of every `$n` placeholder in `text`.
fn placeholders(text: &str) -> BTreeSet<usize> {
    text.split('$')
        .skip(1)
        .filter_map(|tail| {
            let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
        .collect()
}

#[test]
fn every_statement_shares_one_column_list() {
    // The macro is the single source; this pins that every statement
    // actually expands from it rather than carrying a hand-copied prefix.
    for (text, _) in every_text() {
        assert!(text.starts_with(shared_columns!()), "{text}");
    }
    assert!(SELECT_DETAIL.starts_with(shared_columns!()));
}

#[test]
fn only_the_detail_and_thread_reads_pay_for_the_bodies() {
    // The whole reason the listing is its own text: a page of up to two
    // hundred rows must not carry a trigger payload and an agent's full
    // answer per row. The thread pays for them — it IS the expanded read,
    // paged.
    for text in [SELECT_DETAIL, SELECT_THREAD_PAGE, SELECT_THREAD_PAGE_AFTER] {
        assert!(text.contains(body_columns!()), "{text}");
    }
    for shape in listing_shapes() {
        let text = listing_text(shape.0, shape.1, shape.2);
        assert!(!text.contains("request_json"), "{text}");
        assert!(!text.contains("response_text"), "{text}");
    }
}

#[test]
fn the_bodies_follow_every_shared_column() {
    // `EventDetailRow` decodes the shared columns with the LISTING's decoder,
    // which reads by index. That only holds while the bodies come after all
    // fifteen of them — so the ordering is an invariant of the text, not a
    // convention of how it was written.
    for text in [SELECT_DETAIL, SELECT_THREAD_PAGE, SELECT_THREAD_PAGE_AFTER] {
        let bodies = text
            .find("request_json")
            .expect("a bodies-included read selects the trigger payload");
        let last_shared = text
            .find("cost_nanos")
            .expect("every read selects the summed cost");
        assert!(bodies > last_shared);
    }
}

/// A text's placeholders are exactly `$1` to `$n`, where `n` is how many
/// values `History` binds for the shape that picked it: a gap is a parameter
/// Postgres cannot type, and a number past `n` is a value never bound.
#[test]
fn every_text_numbers_what_history_binds() {
    for (text, bound) in every_text() {
        assert_eq!(placeholders(text), (1..=bound).collect(), "{text}");
    }
}

/// Nothing sits behind `IS NULL OR`: a generic plan cannot decide that guard,
/// so a bound behind it becomes a filter, and even a filter-only guard drags
/// the row estimate low enough that the plan sorts instead of walking the
/// index.
#[test]
fn no_predicate_hides_behind_a_null_gate() {
    for (text, _) in every_text() {
        assert!(!text.contains("IS NULL OR"), "{text}");
    }
}

/// Each listing shape carries exactly the predicates it was picked for: the
/// fleet when fleet-scoped, the keyset pair when resuming (RULE KYS), the
/// actor when filtering, and `since` always.
#[test]
fn every_listing_carries_the_predicates_of_its_shape() {
    for shape @ (fleet, resumes, actor) in listing_shapes() {
        let text = listing_text(fleet, resumes, actor);
        assert_eq!(text.contains("AND fleet_id = $2"), fleet, "{shape:?}");
        assert_eq!(
            text.contains("AND (created_at, event_id) < ($"),
            resumes,
            "{shape:?}"
        );
        assert_eq!(text.contains("AND actor LIKE $"), actor, "{shape:?}");
        assert!(text.contains("AND created_at >= $"), "{shape:?}");
    }
    assert!(SELECT_THREAD_PAGE_AFTER.contains("AND (created_at, event_id) < ($3, $4)"));
}

/// The suite that plans the texts plans every one `History` runs.
#[cfg(feature = "test-util")]
#[test]
fn the_planned_texts_are_every_text_history_runs() {
    let planned: BTreeSet<&str> = READ_TEXTS.iter().map(|(_, text)| *text).collect();
    let run: BTreeSet<&str> = every_text().into_iter().map(|(text, _)| text).collect();
    assert_eq!(planned, run);
}
