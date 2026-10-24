use afd_wire::activity::ActivityFrame;
use afr_egress::testing::CountingMint;
use tokio_util::sync::CancellationToken;

use super::{AgentRun, Meter};
use crate::fixture::{API_KEY, GITHUB_TOKEN, lease, unbounded};
use crate::testing::Discard;

#[test]
fn a_run_debugs_without_the_leases_secrets() {
    let lease = lease(&[], unbounded());
    let sink = |_frame: ActivityFrame<'static>| {};
    let run = AgentRun {
        lease: &lease,
        memory: afr_memory::Seed::default(),
        executor: None,
        mint: &CountingMint::never(),
        verbs: &afr_tools::CLOSED,
        checkpoint: &Discard,
        context_checkpoints: &crate::testing::ContextMemory::default(),
        events: &sink,
        meter: &Meter::default(),
        stop: &CancellationToken::new(),
    };

    let rendered = format!("{run:?}");

    assert!(rendered.contains(&*lease.lease_id), "{rendered}");
    for secret in [API_KEY, GITHUB_TOKEN] {
        assert!(
            !rendered.contains(secret),
            "a lease secret must never render: {rendered}"
        );
    }
}

const CACHE_WRITE_FIVE_MINUTE_TOKENS: u32 = 60;

/// One provider turn's usage.
const TURN: afr_providers::Usage = afr_providers::Usage {
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
    let at = afd_core::clock::now();
    let meter = Meter::new(std::sync::Arc::new(afd_core::clock::FixedClock::at(at)));
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

/// A request body never carries more than the wire allows; the rest waits
/// for the next renewal, and the backlog still counts it.
#[test]
fn a_meter_sends_at_most_one_bodys_worth() {
    let meter = Meter::default();
    for _ in 0..=afd_wire::usage::LINES_MAX {
        meter.add(TURN);
    }
    assert_eq!(meter.unrecorded().0.len(), afd_wire::usage::LINES_MAX);
    assert_eq!(
        meter.backlog(),
        afd_wire::usage::LINES_MAX + 1,
        "the backlog counts past one body"
    );
    meter.recorded_through(u32::try_from(afd_wire::usage::LINES_MAX).unwrap_or(u32::MAX) - 1);
    assert_eq!(
        meter.unrecorded().0.len(),
        1,
        "the one past the cap goes next"
    );
    assert_eq!(meter.backlog(), 1);
}

/// A turn that wrote to the provider's cache sends its writes on its line,
/// apart from input and split by lifetime, so the daemon prices each at the
/// tier's write rate; the run's totals keep every count.
#[test]
fn a_meter_line_carries_cache_writes_by_lifetime() {
    let at = afd_core::clock::now();
    let meter = Meter::new(std::sync::Arc::new(afd_core::clock::FixedClock::at(at)));
    let writing = afr_providers::Usage {
        input: 500,
        cached_input: 400,
        cache_write: 5,
        cache_write_5m: u64::from(CACHE_WRITE_FIVE_MINUTE_TOKENS),
        cache_write_1h: 40,
        output: 200,
    };
    meter.add(writing);

    let line = afd_wire::usage::RequestUsage {
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
