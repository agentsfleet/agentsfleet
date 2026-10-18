//! What §7's report suites share: the report they send, and the emptiness two
//! of them assert.
//!
//! Split out because the dimension has two shapes and they belong in separate
//! files — one proves the four writes commit together, the other proves an
//! empty claim commits none of them — while the fixture between them is the
//! same run reported the same way.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use afd_billing::Cumulative;
use afd_core::id::Uuid7;
use afd_fleet::lease::usage::Pricing;
use afd_fleet::lease::{Reported, Terminal, TerminalReport, Verdict};
use jiff::Timestamp;

use crate::report_seed::{DEEP_POOL, Held, SLICE_MS};
use crate::rows::Row;

/// A response Postgres refuses, after the settle has already written.
///
/// A NUL byte in a `text` value is rejected with `22021` — and a runner's
/// response text is bytes nothing upstream validates, so this is a report the
/// platform can actually receive rather than a seam invented for a test. It
/// fails at `UPDATE_FLEET_EVENT_RESULT`, the second statement in the
/// transaction: the charge is already written when it lands, which is exactly
/// the boundary this dimension is about.
pub(crate) const RESPONSE_POSTGRES_REFUSES: &str = "the answer\u{0}with a null in it";

/// The answer an accepted report carries, and the one the row must end up
/// holding.
pub(crate) const RESPONSE_ACCEPTED: &str = "the fleet finished and said this";

/// The event the fleet's next run resumes after.
pub(crate) const RESUME_EVENT_ID: &str = "1788550034853-0";

/// The answer that next run resumes from.
pub(crate) const RESUME_RESPONSE: &str = "resume from here";

/// No usage line: admission charges nothing, and no report has committed.
pub(crate) const LEDGER_ROWS_BEFORE_SETTLE: i64 = 0;

/// One usage line: the report's final slice, however often it is re-sent.
pub(crate) const LEDGER_ROWS_AFTER_SETTLE: i64 = 1;

/// One terminal report over the seeded lease, carrying `response_text`.
///
/// The verdict is clean on every call: what varies is whether Postgres will
/// store the answer and whether the claim can still win. Holding everything
/// else fixed is what makes the wallet assertions read as one run reported
/// more than once.
pub(crate) fn report<'a>(
    lease_id: &'a str,
    runner_id: &'a Uuid7,
    lease: &'a Reported,
    priced: (&'a Uuid7, &'a Pricing),
    response_text: &'a str,
    now: Timestamp,
) -> TerminalReport<'a> {
    let (lease_key, pricing) = priced;
    TerminalReport {
        lease_id,
        runner_id,
        lease,
        lease_key,
        cumulative: Cumulative::default(),
        pricing,
        outcome: Terminal {
            verdict: Verdict::Succeeded,
            response_text,
            tokens: 0,
            wall_ms: SLICE_MS,
            tool_calls: None,
        },
        last_event_id: RESUME_EVENT_ID,
        last_response: RESUME_RESPONSE,
        held_until: None,
        now,
    }
}

/// Not one of the four writes reached the datastore.
///
/// `why` names the route that got here, because three different empty claims
/// share this assertion set and a bare failure would not say which one broke.
pub(crate) async fn assert_nothing_landed(held: &Held, why: &str) {
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(DEEP_POOL),
        "{why}: a tenant charged for a run whose answer was never stored is the failure \
         this dimension names"
    );
    assert_eq!(
        held.fixtures
            .text(Row::Event(&held.fleet, &held.event_id), "status")
            .await,
        Some(afd_core::event::status::RECEIVED.to_owned()),
        "{why}: the event was never ended"
    );
    assert_eq!(
        held.fixtures
            .text(Row::Session(&held.fleet), "context_json")
            .await,
        None,
        "{why}: no cursor was written — a session pointing past a run that did not finish \
         would feed the next run an answer nobody was charged for"
    );
    assert_eq!(
        held.fixtures.ledger_rows(&held.event_id).await,
        LEDGER_ROWS_BEFORE_SETTLE,
        "{why}: no usage line was logged; it belongs to a report that did not commit"
    );
}
