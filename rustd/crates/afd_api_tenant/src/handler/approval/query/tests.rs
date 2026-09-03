//! What the listing parser accepts, and what it refuses.
//!
//! Split from [`super`] at the file cap's first cut — a module's inline tests
//! move before its logic does.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use super::*;

/// A cursor in the form `keyset_cursor.zig` writes.
const ZIG_CURSOR: &str = "1735689600000:01924f4e-0000-7000-8000-00000000a11e";

/// The boundary instant that cursor carries.
const ZIG_CURSOR_AT: i64 = 1_735_689_600_000;

/// The boundary gate that cursor carries.
const ZIG_CURSOR_GATE: &str = "01924f4e-0000-7000-8000-00000000a11e";

/// The same cursor as a browser puts it on the wire.
const ZIG_CURSOR_ENCODED: &str = "1735689600000%3A01924f4e-0000-7000-8000-00000000a11e";

/// What a refused parse answers.
const BAD_REQUEST: u16 = 400;

fn refusal_status(query: &str) -> u16 {
    Listing::parse(query)
        .err()
        .map_or(0, |refusal| refusal.status().as_u16())
}

fn parsed(query: &str) -> Listing {
    Listing::parse(query)
        .ok()
        .unwrap_or_else(|| panic!("{query} should parse"))
}

#[test]
fn an_empty_query_is_the_pending_page_at_the_default_size() {
    let listing = parsed("");
    assert_eq!(listing.limit, DEFAULT_LIMIT);
    assert!(listing.status.is_none(), "absent means pending");
    assert!(listing.fleet_id.is_none());
    assert!(listing.gate_kind.is_none());
    assert!(listing.cursor.is_none());
}

#[test]
fn the_three_filters_are_read_off_the_string() {
    let listing = parsed(&format!(
        "status=denied&fleet_id={ZIG_CURSOR_GATE}&gate_kind=spend"
    ));
    assert_eq!(listing.status, Some(GateStatus::Denied));
    assert_eq!(listing.fleet_id.as_deref(), Some(ZIG_CURSOR_GATE));
    assert_eq!(listing.gate_kind.as_deref(), Some("spend"));
}

#[test]
fn every_state_a_row_can_be_in_maps_to_the_status_it_names() {
    // All five, not the two the other cases happen to reach: `approved` and
    // `timed_out` could be swapped and the integration case that reads an
    // empty page would pass either way. `auto_killed` is the one that was
    // unreachable — a state the column really holds, which the filter refused
    // because it routed through the WRITER's three-arm vocabulary.
    assert_eq!(parsed("status=approved").status, Some(GateStatus::Approved));
    assert_eq!(parsed("status=denied").status, Some(GateStatus::Denied));
    assert_eq!(
        parsed("status=timed_out").status,
        Some(GateStatus::TimedOut)
    );
    assert_eq!(
        parsed("status=auto_killed").status,
        Some(GateStatus::AutoKilled)
    );
    assert_eq!(parsed("status=pending").status, None);
}

#[test]
fn a_cursor_the_dashboard_encoded_still_resumes() {
    // `URLSearchParams` percent-escapes the colon in the clear wire form,
    // so this is what the dashboard actually sends. Read raw it finds no
    // separator, and every page after the first is refused.
    let listing = parsed(&format!("cursor={ZIG_CURSOR_ENCODED}"));
    let resume = listing.cursor.expect("an encoded cursor parses");
    assert_eq!(resume.borrowed().created_at, ZIG_CURSOR_AT);
    assert_eq!(resume.borrowed().gate_id, ZIG_CURSOR_GATE);
}

#[test]
fn a_fleet_id_that_is_not_an_identifier_is_refused_here() {
    // The statement casts this value to `uuid`, so an unchecked one comes
    // back as a cast failure: a 500 and an error log line for a typo.
    for malformed in ["fleet_id=abc", "fleet_id=", "fleet_id=01924f4e-nope"] {
        assert_eq!(refusal_status(malformed), BAD_REQUEST, "{malformed}");
    }
    assert_eq!(
        parsed(&format!("fleet_id={ZIG_CURSOR_GATE}"))
            .fleet_id
            .as_deref(),
        Some(ZIG_CURSOR_GATE)
    );
}

#[test]
fn a_broken_escape_refuses_the_request() {
    assert_eq!(refusal_status("gate_kind=%zz"), BAD_REQUEST);
}

#[test]
fn pending_and_an_absent_status_are_the_same_request() {
    assert_eq!(parsed("status=pending").status, None);
    assert_eq!(parsed("").status, None);
}

#[test]
fn a_status_no_row_can_be_in_is_refused_rather_than_ignored() {
    // Every spelling the column holds is served, so this refuses values that
    // are not states at all. Serving the pending page for one would answer a
    // question the caller did not ask, and read to them as an empty inbox.
    assert_eq!(refusal_status("status=Approved"), BAD_REQUEST);
    assert_eq!(refusal_status("status=elsewhere"), BAD_REQUEST);
    assert_eq!(refusal_status("status="), BAD_REQUEST);
}

#[test]
fn the_page_size_band_is_the_zig_daemons() {
    assert_eq!(parsed("limit=1").limit, 1);
    assert_eq!(parsed("limit=200").limit, MAX_LIMIT);
    for outside in ["limit=0", "limit=201", "limit=-1", "limit=ten", "limit="] {
        assert_eq!(refusal_status(outside), BAD_REQUEST, "{outside}");
    }
}

#[test]
fn a_cursor_the_zig_daemon_minted_resumes_this_one() {
    let listing = parsed(&format!("cursor={ZIG_CURSOR}"));
    let resume = listing.cursor.expect("the cursor parses");
    let borrowed = resume.borrowed();
    assert_eq!(borrowed.created_at, ZIG_CURSOR_AT);
    assert_eq!(borrowed.gate_id, ZIG_CURSOR_GATE);
}

#[test]
fn a_cursor_this_endpoint_never_issued_is_refused() {
    // The text form belongs to a name-ordered walk. This listing orders by
    // instant alone, so honouring one would resume an ordering it never
    // served.
    for unminted in [
        "cursor=s:bmFtZQ:01924f4e-0000-7000-8000-00000000a11e",
        "cursor=notacursor",
        "cursor=1735689600000:",
        "cursor=abc:01924f4e",
        "cursor=",
    ] {
        assert_eq!(refusal_status(unminted), BAD_REQUEST, "{unminted}");
    }
}
