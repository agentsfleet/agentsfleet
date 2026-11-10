//! What a case may claim about the outcomes of its matrix.

use super::harness::{Outcome, Verdict};
use af_stream::test_util::MAX_DATAGRAM_BYTES;

pub(super) fn assert_every_payload_arrived(outcomes: &[Outcome], path: &str) {
    assert!(!outcomes.is_empty(), "{path}: no case ran");
    let failed: Vec<String> = outcomes
        .iter()
        .filter(|outcome| !matches!(outcome.verdict, Verdict::Arrived))
        .map(|outcome| format!("{} bytes: {}", outcome.size, outcome.verdict))
        .collect();
    let largest = outcomes.iter().fold(0, |most, outcome| {
        most.max(outcome.traffic.largest_to_client)
    });
    let dropped: usize = outcomes
        .iter()
        .map(|outcome| outcome.traffic.dropped_to_client + outcome.traffic.dropped_to_server)
        .sum();
    assert!(
        failed.is_empty(),
        "{path} failed {failed:?}; the server's largest datagram was {largest} bytes and the path dropped {dropped}"
    );
}

pub(super) fn assert_nothing_dropped(outcomes: &[Outcome], path: &str) {
    let dropped: Vec<String> = outcomes
        .iter()
        .filter(|outcome| outcome.traffic.dropped_to_client + outcome.traffic.dropped_to_server > 0)
        .map(|outcome| {
            format!(
                "{} bytes: {} toward the client, {} toward the server",
                outcome.size, outcome.traffic.dropped_to_client, outcome.traffic.dropped_to_server
            )
        })
        .collect();
    assert!(dropped.is_empty(), "{path} dropped datagrams: {dropped:?}");
}

pub(super) fn assert_within_ceiling(outcomes: &[Outcome], path: &str) {
    assert!(!outcomes.is_empty(), "{path}: no case ran");
    let over: Vec<String> = outcomes
        .iter()
        .filter(|outcome| {
            outcome.traffic.largest_to_client > MAX_DATAGRAM_BYTES
                || outcome.traffic.largest_to_server > MAX_DATAGRAM_BYTES
        })
        .map(|outcome| {
            format!(
                "{} bytes: server sent {}, client sent {}",
                outcome.size, outcome.traffic.largest_to_client, outcome.traffic.largest_to_server
            )
        })
        .collect();
    assert!(
        over.is_empty(),
        "{path}: datagrams over the {MAX_DATAGRAM_BYTES}-byte ceiling: {over:?}"
    );
}

/// The ceiling costs packets, not a flood of them: the largest echo stays within twice the packet
/// count of kilobyte-sized datagrams.
pub(super) fn assert_datagram_budget(outcomes: &[Outcome], path: &str) {
    let (size, to_client, to_server) = outcomes
        .iter()
        .max_by_key(|outcome| outcome.size)
        .map(|outcome| {
            (
                outcome.size,
                outcome.traffic.datagrams_to_client,
                outcome.traffic.datagrams_to_server,
            )
        })
        .unwrap_or_default();
    assert_ne!(size, 0, "{path}: no case ran");
    let budget = 2 * (size / 1_000 + 1);
    assert!(
        to_client <= budget && to_server <= budget,
        "{path}: {to_client} datagrams toward the client and {to_server} toward the server exceed the budget of {budget} for {size} bytes"
    );
}
