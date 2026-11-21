use af_egress::testing::CountingMint;
use af_wire::activity::ActivityFrame;
use tokio_util::sync::CancellationToken;

use super::{AgentRun, Meter};
use crate::fixture::{API_KEY, GITHUB_TOKEN, payload, unbounded};
use crate::testing::Discard;

#[test]
fn a_run_debugs_without_the_runs_secrets() {
    let this_run = payload(&[], unbounded());
    let sink = |_frame: ActivityFrame<'static>| {};
    let run = AgentRun {
        recovery: None,
        recovered_task: None,
        recovered_turn: None,
        prepared_task: None,
        run: &this_run,
        memory: afr_memory::Seed::default(),
        executor: &afr_executor::ExecutorSlot::unavailable(afr_executor::Unavailable::NoSandbox),
        mint: &CountingMint::never(),
        verbs: &afr_tools::CLOSED,
        checkpoint: &Discard,
        inbox: &crate::testing::NoInbox,
        events: &sink,
        meter: &Meter::default(),
        stop: &CancellationToken::new(),
    };

    let rendered = format!("{run:?}");

    assert!(rendered.contains(&*this_run.run_id), "{rendered}");
    for secret in [API_KEY, GITHUB_TOKEN] {
        assert!(
            !rendered.contains(secret),
            "a this_run secret must never render: {rendered}"
        );
    }
}

const CACHE_WRITE_FIVE_MINUTE_TOKENS: u32 = 60;

/// One provider turn's usage.
const TURN: guffbolt::Usage = guffbolt::Usage {
    input: 30,
    cached_input: 10,
    cache_write: 0,
    cache_write_5m: 0,
    cache_write_1h: 0,
    output: 4,
};

/// Each request is one line, numbered from zero and stamped by the meter's
/// clock; a recorded number forgets the lines through it and no others.
#[test]
fn a_meter_numbers_its_requests_and_forgets_only_what_was_recorded() {
    let at = af_core::clock::now();
    let meter = Meter::new(std::sync::Arc::new(af_core::clock::FixedClock::at(at)));
    for _ in 0..3 {
        meter.add(TURN);
    }
    let sent = meter.unrecorded();
    let numbers: Vec<u32> = sent.0.iter().map(|line| line.seq).collect();
    assert_eq!(numbers, [0, 1, 2]);
    assert!(
        sent.0
            .iter()
            .all(|line| line.at == at && line.input_tokens == 30)
    );
    assert_eq!(meter.read().input, 90, "the totals still add up");

    meter.recorded_through(1);
    let left: Vec<u32> = meter.unrecorded().0.iter().map(|line| line.seq).collect();
    assert_eq!(
        left,
        [2],
        "a later request survives an earlier acknowledgement"
    );
}

/// A request body carries every line the daemon has not recorded, and a
/// recorded line leaves it.
#[test]
fn a_meter_sends_every_unrecorded_line() {
    let meter = Meter::default();
    for _ in 0..1_100 {
        meter.add(TURN);
    }
    assert_eq!(meter.unrecorded().0.len(), 1_100);
    assert_eq!(meter.backlog(), 1_100);
    meter.recorded_through(1_098);
    assert_eq!(meter.unrecorded().0.len(), 1, "only the newest is left");
    assert_eq!(meter.backlog(), 1);
}

/// A turn that wrote to the provider's cache sends its writes on its line,
/// apart from input and split by lifetime, so the daemon prices each at the
/// tier's write rate; the run's totals keep every count.
#[test]
fn a_meter_line_carries_cache_writes_by_lifetime() {
    let at = af_core::clock::now();
    let meter = Meter::new(std::sync::Arc::new(af_core::clock::FixedClock::at(at)));
    let writing = guffbolt::Usage {
        input: 500,
        cached_input: 400,
        cache_write: 5,
        cache_write_5m: u64::from(CACHE_WRITE_FIVE_MINUTE_TOKENS),
        cache_write_1h: 40,
        output: 200,
    };
    meter.add(writing);

    let line = af_wire::usage::RequestUsage {
        seq: 0,
        at,
        input_tokens: 500,
        cached_input_tokens: 400,
        cache_write_tokens: 5,
        cache_write_5m_tokens: CACHE_WRITE_FIVE_MINUTE_TOKENS,
        cache_write_1h_tokens: 40,
        output_tokens: 200,
    };
    assert_eq!(meter.unrecorded().0, [line], "a write is never also input");
    assert_eq!(meter.read(), writing);
    assert_eq!(meter.read().total(), 1_205, "every token once");
}
