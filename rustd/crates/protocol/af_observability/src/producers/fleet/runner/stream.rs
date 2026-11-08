//! Native connection failures stay visible even when fallback succeeds.

use std::time::Duration;

use opentelemetry::{
    KeyValue,
    metrics::{Counter, Histogram},
};

use crate::{
    error::Result,
    metrics::{
        declared::fleet as declared,
        instrument::Instruments,
        label::fleet::{
            ReadyHint, StreamFallback, StreamOutcome, StreamPacket, StreamStage, StreamTransport,
        },
    },
    producers::installed,
};

#[derive(Debug)]
pub(in crate::producers::fleet) struct Handles {
    connections: Counter<u64>,
    connect_seconds: Histogram<f64>,
    operations: Histogram<f64>,
    fallbacks: Counter<u64>,
    timeouts: Counter<u64>,
    hints: Counter<u64>,
    native_rtt: Histogram<f64>,
    native_packets: Counter<u64>,
    native_pto: Counter<u64>,
    native_retransmitted_bytes: Counter<u64>,
}

impl Handles {
    pub(in crate::producers::fleet) fn claim(instruments: &Instruments) -> Result<Self> {
        Ok(Self {
            connections: instruments.counter_u64(&declared::RUNNER_STREAM_CONNECT_TOTAL)?,
            connect_seconds: instruments.histogram_f64(&declared::RUNNER_STREAM_CONNECT_SECONDS)?,
            operations: instruments.histogram_f64(&declared::RUNNER_STREAM_OPERATION_SECONDS)?,
            fallbacks: instruments.counter_u64(&declared::RUNNER_STREAM_FALLBACK_TOTAL)?,
            timeouts: instruments.counter_u64(&declared::RUNNER_STREAM_TIMEOUTS_TOTAL)?,
            hints: instruments.counter_u64(&declared::FLEET_READY_WAKE_HINTS_TOTAL)?,
            native_rtt: instruments.histogram_f64(&declared::RUNNER_STREAM_NATIVE_RTT_SECONDS)?,
            native_packets: instruments
                .counter_u64(&declared::RUNNER_STREAM_NATIVE_PACKETS_TOTAL)?,
            native_pto: instruments.counter_u64(&declared::RUNNER_STREAM_NATIVE_PTO_TOTAL)?,
            native_retransmitted_bytes: instruments
                .counter_u64(&declared::RUNNER_STREAM_NATIVE_RETRANSMITTED_BYTES_TOTAL)?,
        })
    }
}

/// Counts the entire native attempt, rather than only the eventual fallback.
pub fn connected(transport: StreamTransport, outcome: StreamOutcome, elapsed: Duration) {
    if let Some(producers) = installed() {
        let labels = [
            KeyValue::new("transport", transport.as_str()),
            KeyValue::new("outcome", outcome.as_str()),
        ];
        producers.fleet.stream.connections.add(1, &labels);
        producers
            .fleet
            .stream
            .connect_seconds
            .record(elapsed.as_secs_f64(), &labels);
    }
}

/// Records completion, timeout, cancellation and dropped futures under one clock.
pub fn operation(stage: StreamStage, outcome: StreamOutcome, elapsed: Duration) {
    if let Some(producers) = installed() {
        producers.fleet.stream.operations.record(
            elapsed.as_secs_f64(),
            &[
                KeyValue::new("stage", stage.as_str()),
                KeyValue::new("outcome", outcome.as_str()),
            ],
        );
        if outcome == StreamOutcome::Timeout {
            producers
                .fleet
                .stream
                .timeouts
                .add(1, &[KeyValue::new("stage", stage.as_str())]);
        }
    }
}

/// Records the failed preferred path before a fallback connection begins.
pub fn fallback(reason: StreamFallback) {
    if let Some(producers) = installed() {
        producers
            .fleet
            .stream
            .fallbacks
            .add(1, &[KeyValue::new("reason", reason.as_str())]);
    }
}

/// Records bounded hint pressure without treating an eviction as lost work.
pub fn hint(outcome: ReadyHint) {
    if let Some(producers) = installed() {
        producers
            .fleet
            .stream
            .hints
            .add(1, &[KeyValue::new("outcome", outcome.as_str())]);
    }
}

/// Changes since the previous sample from this one connection.
#[derive(Debug, Default, Clone, Copy)]
pub struct NetworkDeltas {
    /// Sent packets, including packets carrying retransmissions.
    pub sent: u64,
    /// Packets marked lost.
    pub lost: u64,
    /// Sent packets carrying retransmitted data.
    pub retransmitted: u64,
    /// Probe timeouts across all paths.
    pub probe_timeouts: u64,
    /// Stream bytes sent again.
    pub retransmitted_bytes: u64,
}

/// Records actual QUIC samples outside the native I/O callbacks.
pub fn native(rtt: Option<Duration>, deltas: NetworkDeltas) {
    if let Some(producers) = installed() {
        let handles = &producers.fleet.stream;
        if let Some(rtt) = rtt {
            handles.native_rtt.record(
                rtt.as_secs_f64(),
                &[KeyValue::new("transport", StreamTransport::Http3.as_str())],
            );
        }
        for (outcome, count) in [
            (StreamPacket::Sent, deltas.sent),
            (StreamPacket::Lost, deltas.lost),
            (StreamPacket::Retransmitted, deltas.retransmitted),
        ] {
            if count > 0 {
                handles
                    .native_packets
                    .add(count, &[KeyValue::new("outcome", outcome.as_str())]);
            }
        }
        if deltas.probe_timeouts > 0 {
            handles.native_pto.add(deltas.probe_timeouts, &[]);
        }
        if deltas.retransmitted_bytes > 0 {
            handles
                .native_retransmitted_bytes
                .add(deltas.retransmitted_bytes, &[]);
        }
    }
}
