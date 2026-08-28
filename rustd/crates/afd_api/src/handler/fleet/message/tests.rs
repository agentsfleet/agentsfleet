//! What a thread page cuts at, and what a steer's body is allowed to be.
//!
//! Both halves of this surface decide something before any datastore is
//! reached: the read decides how many rows a page carries and which row the
//! cursor names, and the write decides whether the bytes a client sent are a
//! message at all. Neither decision needs Postgres or Redis, so both are proven
//! here; `fleet_messages.rs` is left proving the credential, the two rungs and
//! the ownership layer over HTTP.

#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_events::{Cursor, EventDetailRow, THREAD_DEFAULT_LIMIT, THREAD_MAX_LIMIT};
use axum::body::Bytes;

use super::{
    MAX_MESSAGE_BYTES, PAGE_BUDGET_BYTES, included_under_budget, page, parse_cursor, parse_limit,
    read_message,
};

/// The millisecond the fixture thread's oldest row was stamped.
const FIRST_MS: i64 = 1_700_000_000_000;

/// A stream entry id, spelled the way Redis mints one.
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

/// A steer with nothing in it is refused before the parser runs.
#[test]
fn should_refuse_a_steer_that_carries_no_body() {
    read_message(&Bytes::new()).unwrap_err();
}

/// A body this daemon cannot read is refused.
#[test]
fn should_refuse_a_body_that_is_not_a_message() {
    for body in [
        "",
        "{",
        "null",
        "[]",
        r#""hello""#,
        "{}",
        r#"{"message":null}"#,
        r#"{"message":7}"#,
    ] {
        assert!(
            read_message(&Bytes::from(body.as_bytes().to_vec())).is_err(),
            "{body} is not a steer this surface accepts"
        );
    }
}

/// An empty message is refused: a person pressed send on nothing.
#[test]
fn should_refuse_an_empty_message() {
    read_message(&Bytes::from_static(br#"{"message":""}"#)).unwrap_err();
}

/// An escaped message is a message, not a malformed body.
///
/// The regression this file exists for on the write side. `serde` hands back
/// `Cow::Owned` for any string carrying an escape, so a reader that accepted
/// only a borrow would refuse a newline, a quote and an emoji — which is most
/// of what a person types into a chat box.
#[test]
fn should_read_a_message_that_carries_escapes() {
    let body = Bytes::from_static(br#"{"message":"line one\nline \"two\"\tand \u2728 done"}"#);
    assert_eq!(
        read_message(&body).unwrap(),
        "line one\nline \"two\"\tand \u{2728} done",
    );
}

/// The length bound is on the DECODED bytes, not on what a client sent.
///
/// A message of newlines doubles in the encoded form: bounding the escaped
/// bytes would refuse a message half the documented size, and the runner reads
/// the decoded text.
#[test]
fn should_bound_the_decoded_bytes_and_not_the_escaped_ones() {
    let escaped = format!(r#"{{"message":"{}"}}"#, r"\n".repeat(MAX_MESSAGE_BYTES));
    let body = Bytes::from(escaped.into_bytes());
    let read = read_message(&body).expect("a message of newlines is under the bound once decoded");
    assert_eq!(read.len(), MAX_MESSAGE_BYTES);
}

/// The bound admits its own ceiling and refuses one byte past it.
#[test]
fn should_admit_the_ceiling_and_refuse_one_byte_past_it() {
    let at_the_ceiling = format!(r#"{{"message":"{}"}}"#, "a".repeat(MAX_MESSAGE_BYTES));
    read_message(&Bytes::from(at_the_ceiling.into_bytes()))
        .expect("the documented ceiling is a message this surface takes");

    let one_past = format!(r#"{{"message":"{}"}}"#, "a".repeat(MAX_MESSAGE_BYTES + 1));
    read_message(&Bytes::from(one_past.into_bytes())).unwrap_err();
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
