//! The three stored words, and why they are the schema's only enforcement.
//!
//! # These strings are on-disk data, not labels
//!
//! This repository forbids static strings in the schema — no `DEFAULT 'value'`,
//! no `CHECK (col IN (…))` — so the column holding `"active"` is an unconstrained
//! text column and `DesiredStatus::as_str` is the ONLY thing deciding what may
//! go in it. That makes each word below a wire format with rows already written
//! in it: renaming one does not fail a migration, it silently orphans every row
//! carrying the old spelling, and `parse` starts answering `None` for schedules
//! that were fine yesterday.
//!
//! So the tables here pin the literal words. A test that asked only for a
//! round-trip would pass through any rename and prove only that the enum agrees
//! with itself.
//!
//! # Totality is a compile-time property here
//!
//! `parse` is a linear scan of `ALL`, so a variant missing from `ALL` is not a
//! slow parse, it is an unparseable one — and nothing in the type system
//! notices. Each case table below is walked by an exhaustive `match` with no
//! wildcard arm, so ADDING a variant fails to compile here rather than failing
//! a count at run time.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_cron::model::{DEFAULT_TIMEZONE, MAX_SCHEDULES_PER_FLEET};
use afd_cron::{DesiredStatus, Source, SyncStatus, validate};

/// Every source and the word its column holds.
const SOURCES: &[(Source, &str)] = &[(Source::Api, "api"), (Source::Trigger, "trigger")];

/// Every intent and the word its column holds.
const INTENTS: &[(DesiredStatus, &str)] = &[
    (DesiredStatus::Active, "active"),
    (DesiredStatus::Paused, "paused"),
    (DesiredStatus::Deleting, "deleting"),
];

/// Every sync state and the word its column holds.
const SYNC_STATES: &[(SyncStatus, &str)] = &[
    (SyncStatus::Syncing, "syncing"),
    (SyncStatus::Synced, "synced"),
    (SyncStatus::Failed, "failed"),
];

/// A word no variant spells, for the negative leg.
const UNKNOWN: &str = "quiesced";

// ── Totality: the compiler holds this, not an assertion ──────────────────────

#[test]
fn every_source_is_declared_and_spelled_as_this_table_says() {
    for (source, word) in SOURCES {
        // Exhaustive, no wildcard: a new variant fails to COMPILE here.
        match source {
            Source::Api | Source::Trigger => {}
        }
        assert!(
            Source::ALL.contains(source),
            "{source:?} is not in Source::ALL, so parse can never answer it"
        );
        assert_eq!(
            source.as_str(),
            *word,
            "the stored word changed under {source:?}"
        );
    }
    assert_eq!(
        Source::ALL.len(),
        SOURCES.len(),
        "a variant reached ALL without a row in this table"
    );
}

#[test]
fn every_intent_is_declared_and_spelled_as_this_table_says() {
    for (intent, word) in INTENTS {
        match intent {
            DesiredStatus::Active | DesiredStatus::Paused | DesiredStatus::Deleting => {}
        }
        assert!(
            DesiredStatus::ALL.contains(intent),
            "{intent:?} is not in DesiredStatus::ALL"
        );
        assert_eq!(
            intent.as_str(),
            *word,
            "the stored word changed under {intent:?}"
        );
    }
    assert_eq!(
        DesiredStatus::ALL.len(),
        INTENTS.len(),
        "a variant reached ALL without a row in this table"
    );
}

#[test]
fn every_sync_state_is_declared_and_spelled_as_this_table_says() {
    for (state, word) in SYNC_STATES {
        match state {
            SyncStatus::Syncing | SyncStatus::Synced | SyncStatus::Failed => {}
        }
        assert!(
            SyncStatus::ALL.contains(state),
            "{state:?} is not in SyncStatus::ALL"
        );
        assert_eq!(
            state.as_str(),
            *word,
            "the stored word changed under {state:?}"
        );
    }
    assert_eq!(
        SyncStatus::ALL.len(),
        SYNC_STATES.len(),
        "a variant reached ALL without a row in this table"
    );
}

// ── Round trips, both directions ─────────────────────────────────────────────

#[test]
fn a_stored_word_reads_back_as_the_variant_that_wrote_it() {
    for (source, word) in SOURCES {
        assert_eq!(Source::parse(word), Some(*source));
    }
    for (intent, word) in INTENTS {
        assert_eq!(DesiredStatus::parse(word), Some(*intent));
    }
    for (state, word) in SYNC_STATES {
        assert_eq!(SyncStatus::parse(word), Some(*state));
    }
}

#[test]
fn no_two_variants_of_one_enum_share_a_word() {
    let mut words: Vec<&str> = Source::ALL.iter().map(|it| it.as_str()).collect();
    words.sort_unstable();
    let before = words.len();
    words.dedup();
    assert_eq!(
        before,
        words.len(),
        "two sources share a stored word: {words:?}"
    );

    let mut words: Vec<&str> = DesiredStatus::ALL.iter().map(|it| it.as_str()).collect();
    words.sort_unstable();
    let before = words.len();
    words.dedup();
    assert_eq!(
        before,
        words.len(),
        "two intents share a stored word: {words:?}"
    );

    let mut words: Vec<&str> = SyncStatus::ALL.iter().map(|it| it.as_str()).collect();
    words.sort_unstable();
    let before = words.len();
    words.dedup();
    assert_eq!(
        before,
        words.len(),
        "two sync states share a stored word: {words:?}"
    );
}

/// A word from a build that knew more than this one reads as unknown, not as
/// the nearest match.
///
/// The row is not corrupt and the daemon is not wrong — it is older. `None` is
/// what lets a caller say so; a lenient parse would silently treat a state it
/// has never heard of as whichever variant it scanned past first.
#[test]
fn a_word_this_build_does_not_know_parses_as_nothing() {
    assert_eq!(Source::parse(UNKNOWN), None);
    assert_eq!(DesiredStatus::parse(UNKNOWN), None);
    assert_eq!(SyncStatus::parse(UNKNOWN), None);
}

#[test]
fn parsing_is_exact_and_not_merely_a_prefix_or_a_case_fold() {
    assert_eq!(
        DesiredStatus::parse("ACTIVE"),
        None,
        "the column is lowercase"
    );
    assert_eq!(DesiredStatus::parse("activ"), None);
    assert_eq!(
        DesiredStatus::parse("active "),
        None,
        "a padded word is a different word"
    );
    assert_eq!(DesiredStatus::parse(""), None);
}

// ── The one predicate that decides whether a fire wakes anything ─────────────

/// Only `Active` fires, and the other two are DROPPED rather than refused.
///
/// A paused or deleting schedule that fires is the external scheduler not yet
/// knowing what it was last told. The sender is a correctly configured provider
/// acting in good faith, so refusing would earn a retry storm for a state this
/// daemon is about to reconcile away anyway.
#[test]
fn only_an_active_schedule_fires() {
    for (intent, word) in INTENTS {
        assert_eq!(
            intent.fires(),
            *intent == DesiredStatus::Active,
            "`{word}` decides the wrong way about waking a fleet"
        );
    }
}

// ── The constants the rest of the crate reads ────────────────────────────────

/// The default zone must be one `validate` would accept from a person.
///
/// A schedule naming no zone is stored as this value and pushed upstream under
/// it. If the two disagreed, every schedule created without an explicit zone
/// would be rejected at the boundary that is supposed to be its default.
#[test]
fn the_default_timezone_is_one_this_daemon_would_accept() {
    validate::timezone(DEFAULT_TIMEZONE)
        .expect("the default zone must pass the check every other zone passes");
}

/// The ceiling is small enough that a fleet's schedules are a bounded read.
///
/// Not a `> 0` sanity check — that is a constant the compiler already knows and
/// asserting it proves nothing. What matters is that the list endpoint never
/// needs paging: every reader of this table fetches a fleet's schedules whole,
/// and a ceiling that grew into the thousands would turn an unpaged read into
/// a slow one without any single change looking wrong.
#[test]
fn the_per_fleet_ceiling_stays_small_enough_to_read_unpaged() {
    let ceiling = MAX_SCHEDULES_PER_FLEET;
    assert!(
        (1..=100).contains(&ceiling),
        "{ceiling} schedules per fleet: at this size the unpaged list read \
         needs revisiting, not just this bound"
    );
}
