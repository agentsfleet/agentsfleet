//! What the two event listings refuse about the QUERY STRING.
//!
//! Sibling of `workspace_events.rs`, which pins who may read the narrative log
//! and what the path has to say. Split along the axis each proves: that one is
//! the guard, the rung, the ownership layer and the three path segments; this
//! is the five parameters and the two mutual exclusions beyond them.
//!
//! # Why every one of these is worth a case
//!
//! These sentences are already on the wire. `docs/REST_API_DESIGN_GUIDELINES.md`
//! §9 treats a narrowing of a served surface as a breaking change, and the Zig
//! daemon still answers these paths in production — so a value it takes and
//! this port refuses is a regression a dashboard hits, and a value it refuses
//! and this port takes is a filter silently doing nothing. The oracle is
//! `workspaces/events.zig` and `fleets/events.zig`; the strings below are
//! theirs, spelled out rather than imported so that the test and the code under
//! test cannot agree with each other by construction.
//!
//! # The refusal ORDER is part of the surface
//!
//! A request can be wrong several ways at once, and which sentence comes back
//! tells a caller what to fix first: `limit`, then the two exclusions, then the
//! drill-down, then the window and the cursor. Pinned in the cases below that
//! supply two faults together.
//!
//! # What this suite deliberately does not prove
//!
//! No row is read. A parameter that survives every check reaches the production
//! store over a Postgres nobody is listening on and earns a `503`, and that
//! refusal is the evidence it was ACCEPTED rather than quietly dropped. Which
//! rows a `LIKE` pattern or a cursor boundary selects is a live-Postgres fact,
//! and lives in the `#[ignore]`d lane `make test-integration-rustd` runs.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code;
use afd_events::Cursor;
use axum::response::Response;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_t1a1a1a1adecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbad";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2events_input";

/// A well-formed fleet identifier the fixture addresses.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000f1ee";

/// The one rung both listings declare, so nothing here is refused by the rung.
const FLEET_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetRead]);

/// The sentence a page size outside the served band earns.
const DETAIL_LIMIT: &str = "limit must be between 1 and 200";

/// The sentence naming a moment AND a row earns.
const DETAIL_WINDOW_AMBIGUOUS: &str = "since_and_cursor_mutually_exclusive";

/// The sentence naming a glob AND a prefix earns.
const DETAIL_ACTOR_AMBIGUOUS: &str = "actor_and_actor_prefix_mutually_exclusive";

/// The sentence a drill-down that is not an identifier earns.
const DETAIL_FLEET_ID: &str = "fleet_id must be a UUIDv7";

/// The sentence a window this daemon cannot read earns.
const DETAIL_SINCE: &str = "invalid_since_format: use Go-style duration (15s, 30m, 2h, 7d) or RFC 3339 (YYYY-MM-DDTHH:MM:SSZ)";

/// The sentence a cursor this daemon did not mint earns.
const DETAIL_CURSOR: &str = "invalid cursor";

/// The sentence an unreachable datastore earns.
const DETAIL_UNAVAILABLE: &str = "Database unavailable";

/// The instant the harness freezes its clock at, so a window is arithmetic.
const FROZEN_MS: i64 = 1_760_000_000_000;

/// The whole workspace's history, with `suffix` appended verbatim.
fn workspace_history(suffix: &str) -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/events{suffix}")
}

/// One fleet's history, with `suffix` appended verbatim.
fn fleet_history(suffix: &str) -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/events{suffix}")
}

/// One fully authorised read, so what answers is the parameter under test.
async fn authorised(path: &str) -> Response {
    let fleet = Fleet::new().with_person(TENANT_KEY, SUBJECT, FLEET_READ);
    harness::send(&fleet.router(), Method::GET, path, Some(TENANT_KEY), "").await
}

/// Reads a problem document's `detail` back.
async fn detail_of(response: Response) -> String {
    let document = harness::json_body(response).await;
    let detail = document.get("detail").and_then(Value::as_str);
    detail.expect("every refusal carries a detail").to_owned()
}

/// Asserts `suffix` is refused on the workspace listing with `expected`.
///
/// Every refusal on this surface is a malformed REQUEST — the caller wrote
/// something this daemon will not read — so the code is asserted here once
/// rather than restated in a dozen cases.
async fn assert_refused(suffix: &str, expected: &str) {
    let response = authorised(&workspace_history(suffix)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{suffix}");
    let document = harness::json_body(response).await;
    let field = |key: &str| document.get(key).and_then(Value::as_str);
    let seen = (field("error_code"), field("detail"));
    // A query a caller can fix is theirs to fix, never this instance's fault.
    let want = (Some(error_code::INVALID_REQUEST.as_str()), Some(expected));
    assert_eq!(seen, want, "{suffix}");
}

/// Asserts `suffix` survived every check and reached the event store.
async fn assert_accepted(suffix: &str) {
    let response = authorised(&workspace_history(suffix)).await;
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "{suffix}: an accepted parameter reaches the store"
    );
    assert_eq!(detail_of(response).await, DETAIL_UNAVAILABLE, "{suffix}");
}

/// Every page size inside the served band is honoured.
#[tokio::test]
async fn a_page_size_inside_the_served_band_reaches_the_store() {
    // Absent is the default page, and the two ends are the band itself: an
    // off-by-one at either would refuse a size the Zig daemon serves today.
    for suffix in ["", "?limit=1", "?limit=200", "?limit=50"] {
        assert_accepted(suffix).await;
    }
}

/// Every page size outside it is refused, whatever the caller wrote.
///
/// Zero is refused rather than clamped: a caller asking for no rows has made a
/// mistake, and an empty page would read to them as an empty history.
#[tokio::test]
async fn a_page_size_outside_the_served_band_is_refused() {
    let outside = [
        "?limit=0",
        "?limit=201",
        "?limit=-1",
        "?limit=abc",
        // Written by a form field the user left blank.
        "?limit=",
        // Past the width the parameter is read at, which is a different
        // failure from being past the band and must not answer differently.
        "?limit=9223372036854775808",
        "?limit=1.5",
        "?limit=1e2",
    ];
    for suffix in outside {
        assert_refused(suffix, DETAIL_LIMIT).await;
    }
}

/// The page size is refused before the exclusions are even looked at.
///
/// Three faults in one request, and the caller is told about the first. The
/// order is `events.zig`'s, and it is what a client's error handling branches
/// on when it retries with one parameter changed.
#[tokio::test]
async fn the_page_size_is_refused_before_the_exclusions_are_looked_at() {
    assert_refused("?limit=0&cursor=zzz&since=garbage", DETAIL_LIMIT).await;
}

/// A cursor this daemon minted resumes the walk.
///
/// Round-tripped through [`Cursor`] rather than pasted, so the case cannot go
/// stale against a change to the wire form: whatever the type emits is what a
/// client sends back, and the parser has to take it.
#[tokio::test]
async fn a_cursor_this_daemon_minted_resumes_the_walk() {
    let minted = Cursor::after(FROZEN_MS, "1785699668169-0").encode();
    assert_accepted(&format!("?cursor={minted}")).await;
}

/// A cursor this daemon did not mint is refused, and says nothing more.
///
/// One sentence for every failure. A parser that told "not base64" from "no
/// separator" from "identifier too long" apart would be describing this
/// daemon's cursor format to whoever was probing it.
#[tokio::test]
async fn a_cursor_this_daemon_did_not_mint_is_refused() {
    let unminted = [
        // Outside the base64url alphabet entirely.
        "?cursor=not-base64!!",
        // An empty value, which a client sends when its paging state is unset.
        "?cursor=",
        // Decodes cleanly to `no-separator`, which carries no boundary.
        "?cursor=bm8tc2VwYXJhdG9y",
        // Decodes to `notanumber:01HZQ` — a separator, an unreadable instant.
        "?cursor=bm90YW51bWJlcjowMUhaUQ",
        // Decodes to `1735689600000:` — a boundary with no row to break the tie.
        "?cursor=MTczNTY4OTYwMDAwMDo",
        // Decodes to `1:` followed by 129 bytes of identifier. The bound is on
        // the DECODED text, so a short base64 string cannot buy a long one.
        "?cursor=MTp4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHg",
    ];
    for suffix in unminted {
        assert_refused(suffix, DETAIL_CURSOR).await;
    }
}

/// Every window form this daemon reads is accepted.
#[tokio::test]
async fn every_window_form_this_daemon_reads_is_accepted() {
    // The four units `parseSince` takes, the zero window that means "now", and
    // the absolute form at exactly the length the Zig shape-checks for.
    for suffix in [
        "?since=15s",
        "?since=30m",
        "?since=2h",
        "?since=7d",
        "?since=0s",
        "?since=2025-01-01T00:00:00Z",
    ] {
        assert_accepted(suffix).await;
    }
}

/// A window this daemon cannot read is refused.
///
/// The offset and fractional forms are refused because `parseRfc3339Z` refuses
/// them: taking them here would make this port's accepted set WIDER than the
/// daemon still serving these paths, which is the migration hazard §9 is about
/// in the other direction. The impossible calendar date is the one declared
/// narrowing — the Zig rolls `2026-02-31` into March, and this refuses it.
#[tokio::test]
async fn a_window_this_daemon_cannot_read_is_refused() {
    let unreadable = [
        "?since=-5m",
        "?since=s",
        "?since=",
        "?since=garbage",
        "?since=2025-01-01T00:00:00",
        "?since=2025-01-01T00:00:00+00:00",
        "?since=2025-01-01T00:00:00.5Z",
        "?since=2026-02-31T00:00:00Z",
    ];
    for suffix in unreadable {
        assert_refused(suffix, DETAIL_SINCE).await;
    }
}

/// Either actor spelling on its own narrows the page.
///
/// Two parameters for one filter, and they are not redundant: under prefix mode
/// a literal `*` matches a literal `*`, where the glob translates it.
#[tokio::test]
async fn either_actor_spelling_alone_narrows_the_page() {
    for suffix in [
        "?actor=steer:*",
        "?actor=webhook:github",
        "?actor_prefix=webhook:",
        "?actor_prefix=",
    ] {
        assert_accepted(suffix).await;
    }
}

/// Naming both actor spellings is refused before the store.
#[tokio::test]
async fn naming_both_actor_spellings_is_refused_before_the_store() {
    assert_refused("?actor=steer:*&actor_prefix=steer:", DETAIL_ACTOR_AMBIGUOUS).await;
}

/// A cursor and a window together are refused before the store is asked.
///
/// They answer the same question two ways — one names a moment, the other names
/// a row — and honouring both means guessing which the caller meant. The second
/// case pins the order: the window exclusion is answered before the actor one,
/// so a request carrying both ambiguities gets one stable sentence.
#[tokio::test]
async fn a_cursor_and_a_window_together_are_refused_before_the_store_is_asked() {
    let minted = Cursor::after(FROZEN_MS, "1785699668169-0").encode();
    let both = format!("?cursor={minted}&since=2h");
    assert_refused(&both, DETAIL_WINDOW_AMBIGUOUS).await;
    let every_ambiguity = "?actor=a&actor_prefix=b&cursor=x&since=1h";
    assert_refused(every_ambiguity, DETAIL_WINDOW_AMBIGUOUS).await;
}

/// The console's drill-down narrows the workspace listing, or is refused.
///
/// `fleet_id=` is what the Live Wall sends when an operator clicks one fleet,
/// and it binds the same argument the per-fleet route binds — so the two cannot
/// answer differently about one fleet's history.
#[tokio::test]
async fn the_drill_down_narrows_the_workspace_listing_or_is_refused() {
    assert_accepted(&format!("?fleet_id={FLEET}")).await;
    assert_refused("?fleet_id=not-a-uuid", DETAIL_FLEET_ID).await;
    // And it is validated BEFORE the window, which is where
    // `workspaces/events.zig` validates it.
    assert_refused("?fleet_id=not-a-uuid&since=garbage", DETAIL_FLEET_ID).await;
}

/// The drill-down is not a parameter the per-fleet listing reads.
///
/// The fleet is already in the path there, so a `fleet_id=` in the query has
/// nothing to narrow and is ignored rather than refused — `fleets/events.zig`
/// never asks the query string for one. Worth pinning because the two listings
/// share a parameter reader, and a reader that validated the drill-down for
/// both would start refusing a request this surface has always served.
#[tokio::test]
async fn the_drill_down_is_not_a_parameter_the_per_fleet_listing_reads() {
    let response = authorised(&fleet_history("?fleet_id=not-a-uuid")).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(detail_of(response).await, DETAIL_UNAVAILABLE);
}

/// Both listings answer one parser, parameter for parameter.
///
/// The property the port is built on — one `Params` read by two entry points —
/// asserted rather than assumed. Two hand-written parsers is what the Zig has,
/// and it is why its two handlers carry copies of `prefixToLike` that had
/// already drifted apart on the backslash.
#[tokio::test]
async fn both_listings_answer_one_parser() {
    let shared = [
        ("?limit=0", DETAIL_LIMIT),
        ("?since=garbage", DETAIL_SINCE),
        ("?cursor=not-base64!!", DETAIL_CURSOR),
        ("?actor=a&actor_prefix=b", DETAIL_ACTOR_AMBIGUOUS),
        ("?cursor=x&since=1h", DETAIL_WINDOW_AMBIGUOUS),
    ];
    for (suffix, expected) in shared {
        let per_fleet = authorised(&fleet_history(suffix)).await;
        assert_eq!(per_fleet.status(), StatusCode::BAD_REQUEST, "{suffix}");
        assert_eq!(detail_of(per_fleet).await, expected, "{suffix}");
        // The workspace half of the same pair, so the two are compared rather
        // than each merely being checked against the same written-down string.
        assert_refused(suffix, expected).await;
    }
}
