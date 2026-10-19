//! A run that ends with more usage lines than one report carries: the settle
//! renews them away first, so the report never drops one.

use std::sync::atomic::AtomicUsize;

use afd_wire::report::{RenewRequest, RenewResponse};
use afd_wire::usage::{LINES_MAX, RequestUsage};
use afr_agent::scripted::{ScriptedEngine, Step};
use afr_providers::Usage;

use super::*;
use crate::test_support::{GRANTED_UNTIL, json};

/// More lines than two bodies carry, so the flush takes two renewals and the
/// report the remainder.
const PAST_TWO_BODIES: usize = 2 * LINES_MAX + 76;

/// One provider request's tokens.
const REQUEST: Usage = Usage {
    input: 5,
    cached_input: 0,
    cache_write: 0,
    cache_write_5m: 0,
    cache_write_1h: 0,
    output: 2,
};

/// A turn that makes `requests` provider requests and answers nothing.
fn spending(requests: usize) -> ScriptedEngine {
    ScriptedEngine::new(std::iter::repeat_n(Step::Spend(REQUEST), requests))
}

/// The lines one renewal carried.
fn renewed(call: &Call) -> Vec<RequestUsage> {
    serde_json::from_slice::<RenewRequest>(call.body.as_ref().unwrap())
        .unwrap()
        .requests
        .0
}

/// The lines the last report carried.
fn reported_lines(calls: &[Call]) -> Vec<RequestUsage> {
    serde_json::from_value(reported(calls)[REQUESTS].clone()).unwrap()
}

/// Every renewal's lines, in the order they were sent.
fn renewals(calls: &[Call]) -> Vec<Vec<RequestUsage>> {
    calls
        .iter()
        .filter(|call| call.verb == Verb::Renew)
        .map(renewed)
        .collect()
}

/// The numbers `lines` carry.
fn numbers<'a>(lines: impl IntoIterator<Item = &'a RequestUsage>) -> Vec<u32> {
    lines.into_iter().map(|line| line.seq).collect()
}

/// `0..count`, as line numbers.
fn first(count: usize) -> Vec<u32> {
    (0..u32::try_from(count).unwrap()).collect()
}

/// A daemon renewal that records every line its body carried.
fn recorded(call: &Call) -> Answer {
    json(&RenewResponse {
        lease_expires_at: GRANTED_UNTIL,
        recorded_through: numbers(&renewed(call)).into_iter().max(),
    })
}

/// A run past two bodies sends its oldest lines through two full renewals and
/// the rest in its report: every line reaches the daemon, none twice, each
/// stamped by the lessee's clock.
#[tokio::test(start_paused = true)]
async fn a_run_past_two_bodies_of_usage_delivers_every_line() {
    let answer = daemon(|call: &Call| (call.verb == Verb::Renew).then(|| recorded(call)));
    let mut rig = Rig::running(answer, FakeEngine::default(), spending(PAST_TWO_BODIES));

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let calls = rig.calls();
    let flushed = renewals(&calls);
    let report = reported_lines(&calls);
    assert_eq!(
        flushed.iter().map(Vec::len).collect::<Vec<_>>(),
        [LINES_MAX, LINES_MAX],
        "two full renewals before the report"
    );
    assert_eq!(report.len(), PAST_TWO_BODIES - 2 * LINES_MAX);
    let delivered = numbers(flushed.iter().flatten().chain(&report));
    assert_eq!(
        delivered,
        first(PAST_TWO_BODIES),
        "every line, oldest first, none sent twice"
    );
    let last_renewal = calls.iter().rposition(|call| call.verb == Verb::Renew);
    assert!(last_renewal < position(&calls, Verb::Report));
    assert!(
        report
            .iter()
            .all(|line| line.at == jiff::Timestamp::UNIX_EPOCH),
        "lines are stamped by the injected clock"
    );
}

/// A renewal refused mid-flush stops it. The report carries the oldest lines
/// the daemon has not recorded, none it already recorded, and the newest go
/// unbilled.
#[tokio::test(start_paused = true)]
async fn a_renewal_refused_mid_flush_reports_the_oldest_unrecorded_lines() {
    let answered = AtomicUsize::new(0);
    let answer = daemon(move |call: &Call| {
        (call.verb == Verb::Renew).then(|| {
            if answered.fetch_add(1, Ordering::SeqCst) == 0 {
                recorded(call)
            } else {
                Answer::Fail(error::refused(Verb::Renew, 409, None))
            }
        })
    });
    let mut rig = Rig::running(answer, FakeEngine::default(), spending(PAST_TWO_BODIES));

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let calls = rig.calls();
    let flushed = renewals(&calls);
    assert_eq!(
        flushed.len(),
        2,
        "one recorded, one refused, then the report"
    );
    assert_eq!(
        numbers(&flushed[1]),
        numbers(&reported_lines(&calls)),
        "the report carries what the refused renewal did not record"
    );
    assert_eq!(
        numbers(&reported_lines(&calls)),
        first(2 * LINES_MAX)[LINES_MAX..],
        "none the first renewal recorded"
    );
}

/// A daemon that answers yet records nothing would otherwise be asked
/// forever: the flush gives up after one renewal and the report carries one
/// body's worth.
#[tokio::test(start_paused = true)]
async fn a_renewal_that_records_nothing_ends_the_flush() {
    let mut rig = Rig::running(
        daemon(healthy),
        FakeEngine::default(),
        spending(LINES_MAX + 1),
    );

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let calls = rig.calls();
    assert_eq!(renewals(&calls).len(), 1, "one renewal, not a spin");
    assert_eq!(numbers(&reported_lines(&calls)), first(LINES_MAX));
}

/// A run within one body renews nothing extra before its report.
#[tokio::test(start_paused = true)]
async fn a_run_within_one_body_reports_without_a_flush() {
    let mut rig = Rig::running(daemon(healthy), FakeEngine::default(), spending(LINES_MAX));

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let calls = rig.calls();
    assert_eq!(renewals(&calls).len(), 0, "no renewal before the report");
    assert_eq!(numbers(&reported_lines(&calls)), first(LINES_MAX));
}
