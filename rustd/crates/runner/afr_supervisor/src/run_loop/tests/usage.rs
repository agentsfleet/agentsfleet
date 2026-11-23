//! A run's report carries every usage line the daemon has not recorded,
//! however many: no renewal is sent first to make room.

use af_wire::usage::RequestUsage;
use afr_agent::scripted::{ScriptedEngine, Step};
use guffbolt::Usage;

use super::*;

/// More lines than the old 512-line body held, twice over.
const MANY: usize = 1_100;

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
    serde_json::from_slice::<af_wire::protocol::Renewal<'_>>(call.body.as_ref().unwrap())
        .unwrap()
        .request
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

/// A run of 1,100 provider requests sends no renewal before its report, and
/// the report carries every line, oldest first, each stamped by the lessee's
/// clock.
#[tokio::test(start_paused = true)]
async fn a_run_of_many_requests_reports_every_line_at_once() {
    let mut rig = Rig::running(daemon(healthy), FakeEngine::default(), spending(MANY));

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

    let calls = rig.calls();
    assert_eq!(renewals(&calls).len(), 0, "no renewal before the report");
    let report = reported_lines(&calls);
    assert_eq!(numbers(&report), first(MANY), "every line, oldest first");
    assert!(
        report
            .iter()
            .all(|line| line.at == jiff::Timestamp::UNIX_EPOCH),
        "lines are stamped by the injected clock"
    );
}
