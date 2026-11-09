//! Real SDK exports retain each finite stream outcome and its elapsed time.

use std::time::Duration;

use af_observability::{
    metrics::label::fleet::{
        StreamFallback, StreamOutcome, StreamStage, StreamTransport, WakeQueueOutcome,
    },
    producers::fleet::runner::stream,
    test_util::Capture,
};

#[test]
fn every_stream_measurement_exports_under_its_declared_labels() {
    let captured = Capture::install();
    for transport in StreamTransport::ALL {
        for outcome in StreamOutcome::ALL {
            stream::connected(*transport, *outcome, Duration::from_millis(400));
            let labels = [
                ("transport", transport.as_str()),
                ("outcome", outcome.as_str()),
            ];
            assert_eq!(
                captured.sum("agentsfleet_runner_stream_connect_total", &labels),
                1
            );
            assert_eq!(
                captured.histogram_count("agentsfleet_runner_stream_connect_seconds", &labels),
                1
            );
            assert_eq!(
                captured.histogram_sum("agentsfleet_runner_stream_connect_seconds", &labels),
                0.4
            );
        }
    }
    assert_operations(&captured);
    assert_native_samples(&captured);
    for reason in StreamFallback::ALL {
        stream::fallback(*reason);
        assert_eq!(
            captured.sum(
                "agentsfleet_runner_stream_fallback_total",
                &[("reason", reason.as_str())]
            ),
            1
        );
    }
    for outcome in WakeQueueOutcome::ALL {
        stream::wake_queue(*outcome);
        assert_eq!(
            captured.sum(
                "agentsfleet_fleet_ready_wake_queue_events_total",
                &[("outcome", outcome.as_str())]
            ),
            1
        );
    }
}

fn assert_operations(captured: &Capture) {
    for stage in StreamStage::ALL {
        for outcome in StreamOutcome::ALL {
            stream::operation(*stage, *outcome, Duration::from_millis(500));
            let labels = [("stage", stage.as_str()), ("outcome", outcome.as_str())];
            assert_eq!(
                captured.histogram_count("agentsfleet_runner_stream_operation_seconds", &labels),
                1
            );
            assert_eq!(
                captured.histogram_sum("agentsfleet_runner_stream_operation_seconds", &labels),
                0.5
            );
        }
        assert_eq!(
            captured.sum(
                "agentsfleet_runner_stream_timeouts_total",
                &[("stage", stage.as_str())]
            ),
            1
        );
    }
}

fn assert_native_samples(captured: &Capture) {
    stream::native(None, stream::NetworkDeltas::default());
    assert_eq!(
        captured.histogram_count(
            "agentsfleet_runner_stream_native_rtt_seconds",
            &[("transport", "http3")]
        ),
        0
    );
    stream::native(
        Some(Duration::from_millis(25)),
        stream::NetworkDeltas {
            sent: 100,
            lost: 3,
            retransmitted: 2,
            probe_timeouts: 1,
            retransmitted_bytes: 512,
        },
    );
    assert_eq!(
        captured.histogram_sum(
            "agentsfleet_runner_stream_native_rtt_seconds",
            &[("transport", "http3")]
        ),
        0.025
    );
    assert_eq!(
        captured.histogram_count(
            "agentsfleet_runner_stream_native_rtt_seconds",
            &[("transport", "http2")]
        ),
        0
    );
    assert_native_counters(captured);
}

fn assert_native_counters(captured: &Capture) {
    for (outcome, value) in [("sent", 100), ("lost", 3), ("retransmitted", 2)] {
        assert_eq!(
            captured.sum(
                "agentsfleet_runner_stream_native_packets_total",
                &[("outcome", outcome)]
            ),
            value
        );
    }
    assert_eq!(
        captured.sum("agentsfleet_runner_stream_native_pto_total", &[]),
        1
    );
    assert_eq!(
        captured.sum(
            "agentsfleet_runner_stream_native_retransmitted_bytes_total",
            &[]
        ),
        512
    );
}
