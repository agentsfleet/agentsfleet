//! What the HTTP surface and the exporter record about themselves.

use std::sync::Arc;

use opentelemetry::KeyValue;
use opentelemetry::metrics::Counter;

use crate::error::Result;
use crate::metrics::declared::http as declared;
use crate::metrics::instrument::{Instruments, Reading};
use crate::metrics::label::http::{DiscardReason, OmissionReason, OmittedAttribute, Signal};
use crate::producers::{GaugeSources, installed};
use crate::semconv;

/// What a `worker_running` gauge answers while the process is alive.
///
/// The callback runs inside this process, so the only reading it can honestly
/// take is that the process is up. `0` is what an ABSENT series means to the
/// alert that reads it, which is why nothing here ever publishes one.
const RUNNING: u64 = 1;

/// The instruments the HTTP surface and the exporter record through.
#[derive(Debug)]
pub struct Handles {
    api_shed: Counter<u64>,
    stream_shed: Counter<u64>,
    frames_dropped: Counter<u64>,
    hub_reconnects: Counter<u64>,
    entries_discarded: Counter<u64>,
    attributes_omitted: Counter<u64>,
}

impl Handles {
    /// Claims every instrument this domain records through.
    ///
    /// # Errors
    ///
    /// Whatever [`Instruments`] refuses — see [`super::install`].
    pub(super) fn claim(instruments: &Instruments, sources: &GaugeSources) -> Result<Self> {
        let handles = Self {
            api_shed: instruments.counter_u64(&declared::API_BACKPRESSURE_REJECTIONS_TOTAL)?,
            stream_shed: instruments.counter_u64(&declared::SSE_BACKPRESSURE_REJECTIONS_TOTAL)?,
            frames_dropped: instruments.counter_u64(&declared::SSE_DROPPED_FRAMES_TOTAL)?,
            hub_reconnects: instruments.counter_u64(&declared::SSE_HUB_RECONNECTS_TOTAL)?,
            entries_discarded: instruments.counter_u64(&declared::OTLP_ENTRIES_DISCARDED_TOTAL)?,
            attributes_omitted: instruments.counter_u64(&declared::OTEL_ATTRIBUTE_OMITTED_TOTAL)?,
        };

        let in_flight = Arc::clone(&sources.requests_in_flight);
        instruments.gauge_u64(&declared::API_IN_FLIGHT_REQUESTS, move || {
            in_flight().into_iter().map(Reading::unlabelled).collect()
        })?;

        let streams = Arc::clone(&sources.streams_in_flight);
        instruments.gauge_u64(&declared::SSE_IN_FLIGHT_STREAMS, move || {
            streams().into_iter().map(Reading::unlabelled).collect()
        })?;

        instruments.gauge_u64(&declared::WORKER_RUNNING, || {
            vec![Reading::unlabelled(RUNNING)]
        })?;

        Ok(handles)
    }
}

/// Records a request refused at the in-flight ceiling.
pub fn request_shed() {
    if let Some(producers) = installed() {
        producers.http.api_shed.add(1, &[]);
    }
}

/// Records an event stream refused at the stream ceiling.
pub fn stream_shed() {
    if let Some(producers) = installed() {
        producers.http.stream_shed.add(1, &[]);
    }
}

/// Records a frame the multiplex could not route to a reader.
pub fn frame_dropped() {
    if let Some(producers) = installed() {
        producers.http.frames_dropped.add(1, &[]);
    }
}

/// Records the pub/sub connection having been re-established.
pub fn hub_reconnected() {
    if let Some(producers) = installed() {
        producers.http.hub_reconnects.add(1, &[]);
    }
}

/// Records telemetry lost before it reached a collector.
///
/// Counted at the SOURCE and in the unit an operator can act on: `count` is
/// entries — spans, records, data points — rather than batches, because a
/// batch count moves with load and says nothing about how much is missing.
pub fn export_discarded(signal: Signal, reason: DiscardReason, count: u64) {
    if let Some(producers) = installed() {
        producers.http.entries_discarded.add(
            count,
            &[
                KeyValue::new(semconv::LABEL_SIGNAL, signal.as_str()),
                KeyValue::new(semconv::LABEL_REASON, reason.as_str()),
            ],
        );
    }
}

/// Records an attribute this process declined to put on a data point.
///
/// The omission is counted precisely so it is not invisible: a model or
/// provider that stops being attributed looks, on a dashboard, exactly like
/// one nobody used.
pub fn attribute_omitted(attribute: OmittedAttribute, reason: OmissionReason) {
    if let Some(producers) = installed() {
        producers.http.attributes_omitted.add(
            1,
            &[
                KeyValue::new(semconv::LABEL_ATTRIBUTE, attribute.as_str()),
                KeyValue::new(semconv::LABEL_REASON, reason.as_str()),
            ],
        );
    }
}
