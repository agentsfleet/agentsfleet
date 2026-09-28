//! What a thread page cuts at.
//!
//! The read decides how many rows a page carries and which row the cursor
//! names before any datastore is reached, so it is proven here;
//! `fleet_messages.rs` is left proving the credential, the two rungs and the
//! ownership layer over HTTP. The write's body is `message_steer/tests.rs`.

#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_events::{Cursor, EventDetailRow, THREAD_DEFAULT_LIMIT, THREAD_MAX_LIMIT};

use super::{PAGE_BUDGET_BYTES, included_under_budget, page, parse_cursor, parse_limit};

/// The millisecond the fixture thread's oldest row was stamped.
const FIRST_MS: i64 = 1_700_000_000_000;

/// A stream entry id, spelled the way Dragonfly mints one.
fn entry_id(ordinal: i64) -> String {
    format!("{}-0", FIRST_MS + ordinal)
}

/// One row whose answer costs `response_bytes` before escaping.
fn row(ordinal: i64, response_bytes: usize) -> EventDetailRow {
    EventDetailRow::fixture(
        &entry_id(ordinal),
        FIRST_MS + ordinal,
        "a".repeat(response_bytes),
    )
}

/// A thread of `count` rows, each cheap enough that only the row cap can cut.
fn cheap_thread(count: i64) -> Vec<EventDetailRow> {
    (0..count).map(|ordinal| row(ordinal, 2)).collect()
}

/// A caller who names no page size gets the served default.
#[test]
fn should_page_at_the_default_when_no_size_is_named() {
    assert_eq!(parse_limit(None).unwrap(), THREAD_DEFAULT_LIMIT);
}

/// Both ends of the served band are accepted.
#[test]
fn should_accept_both_ends_of_the_served_band() {
    assert_eq!(parse_limit(Some("1")).unwrap(), 1);
    assert_eq!(
        parse_limit(Some(&THREAD_MAX_LIMIT.to_string())).unwrap(),
        THREAD_MAX_LIMIT,
    );
}

/// A size outside the band is refused rather than clamped.
///
/// Zero is the one worth naming: clamping it up would answer a page the caller
/// did not ask for, and clamping it down would answer an empty page that reads
/// exactly like a thread with nothing in it.
#[test]
fn should_refuse_a_size_outside_the_band_rather_than_clamp_it() {
    // These are the bytes a caller sends, not a value this daemon holds, so
    // naming them would name nothing.
    // pin test: literal is the contract
    for asked in ["0", "26", "-1", "1000", "", " 5", "5.0", "five", "0x10"] {
        assert!(
            parse_limit(Some(asked)).is_err(),
            "{asked} is not a page size this surface serves"
        );
    }
}

/// A cursor is optional, and one this walk minted comes back whole.
#[test]
fn should_carry_a_cursor_this_walk_minted_back_whole() {
    assert_eq!(parse_cursor(None).unwrap(), None);

    let issued = Cursor::after(FIRST_MS, &entry_id(7));
    let read = parse_cursor(Some(&issued.encode()))
        .unwrap()
        .expect("a cursor this walk minted decodes");
    assert_eq!(read, issued);
}

/// A continuation this walk did not issue is refused.
#[test]
fn should_refuse_a_continuation_this_walk_did_not_issue() {
    for forged in ["not-a-cursor", "!!!!", "MTcwMDAwMDAwMDAwMA"] {
        assert!(
            parse_cursor(Some(forged)).is_err(),
            "{forged} is not a cursor this daemon minted"
        );
    }
}

/// A thread with nothing in it includes nothing.
#[test]
fn should_include_nothing_from_an_empty_thread() {
    assert_eq!(included_under_budget(&[], THREAD_MAX_LIMIT), 0);
}

/// The row cap cuts a page of cheap rows, and only the cap.
#[test]
fn should_cut_a_page_of_cheap_rows_at_the_row_cap() {
    let thread = cheap_thread(3);
    assert_eq!(included_under_budget(&thread, 2), 2);
    assert_eq!(included_under_budget(&thread, THREAD_MAX_LIMIT), 3);
}

/// A page size of zero or less includes nothing rather than the first row.
///
/// The first-row exemption is about the BUDGET, never about the cap: a caller
/// who asked for no rows must not be handed one because it was free.
#[test]
fn should_include_nothing_when_the_cap_admits_nothing() {
    let thread = cheap_thread(3);
    assert_eq!(included_under_budget(&thread, 0), 0);
    assert_eq!(included_under_budget(&thread, -1), 0);
}

/// The first row ships whatever it costs; the second does not.
///
/// A single turn larger than the whole budget must not brick the thread it
/// heads — the operator would see an empty page and no way to page past it.
#[test]
fn should_ship_the_first_row_whatever_it_costs() {
    let thread: Vec<EventDetailRow> = (0..3)
        .map(|ordinal| row(ordinal, PAGE_BUDGET_BYTES))
        .collect();
    assert_eq!(included_under_budget(&thread, THREAD_MAX_LIMIT), 1);
}

/// Rows join until the budget is spent, and the cut is the budget's.
///
/// Six quarter-budget rows against a cap of twenty-five: three fit whatever the
/// JSON envelope costs, and a fourth cannot however small it is, so the number
/// is a fact about the budget rather than about this fixture's escaping.
#[test]
fn should_join_rows_until_the_budget_is_spent() {
    let quarter = PAGE_BUDGET_BYTES / 4;
    let thread: Vec<EventDetailRow> = (0..6).map(|ordinal| row(ordinal, quarter)).collect();
    assert!(i64::try_from(thread.len()).unwrap() < THREAD_MAX_LIMIT);
    assert_eq!(included_under_budget(&thread, THREAD_MAX_LIMIT), 3);
}

/// A page that served everything fetched hands back no continuation.
#[test]
fn should_hand_back_no_continuation_when_the_thread_ended() {
    let thread = cheap_thread(3);
    let served = page(&thread, THREAD_MAX_LIMIT);
    assert_eq!(served.items.len(), 3);
    assert_eq!(served.next_cursor, None);
    assert_eq!(served.total, None);
}

/// The continuation names the LAST ROW SERVED, never the one held back.
///
/// The handler fetches one row more than it serves, so the tail of `fetched` is
/// the row the NEXT page must start at. A cursor minted from it would resume
/// strictly after that row and skip it — a hole in the thread that no client
/// could see, because every page would look complete.
#[test]
fn should_continue_from_the_last_row_served() {
    let thread = cheap_thread(3);
    let served = page(&thread, 2);
    assert_eq!(served.items.len(), 2);

    let handed = served
        .next_cursor
        .expect("a page holding a row back hands back a continuation");
    let resume = Cursor::decode(&handed).expect("the page mints a cursor this walk reads");
    assert_eq!(resume, Cursor::after(FIRST_MS + 1, &entry_id(1)));
}

/// A page the BUDGET cut continues from the cut, not from the row cap.
#[test]
fn should_continue_from_the_budget_cut() {
    let thread: Vec<EventDetailRow> = (0..3)
        .map(|ordinal| row(ordinal, PAGE_BUDGET_BYTES))
        .collect();
    let served = page(&thread, THREAD_MAX_LIMIT);
    assert_eq!(served.items.len(), 1);

    let handed = served
        .next_cursor
        .expect("a page the budget cut has more to serve");
    let resume = Cursor::decode(&handed).expect("the page mints a cursor this walk reads");
    assert_eq!(resume, Cursor::after(FIRST_MS, &entry_id(0)));
}
