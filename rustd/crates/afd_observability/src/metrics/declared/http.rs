//! What the HTTP surface and the exporter say about themselves.
//!
//! The two ceilings this instance serves under — requests in flight and streams
//! carried — plus what each sheds when it meets them, and the exporter's own
//! account of what it could not deliver.
//!
//! The exporter families are here rather than beside the export code because
//! they are what an operator reads when the export is the thing that failed,
//! and they answer the same question the shed counters do: what did this
//! process refuse, and why.

use crate::metrics::family::{CounterKind, Declared, GaugeKind};

/// Any growth: requests shed at the cap.
pub const API_BACKPRESSURE_REJECTIONS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_api_backpressure_rejections_total");

/// Approaching `api_max_in_flight_requests`.
pub const API_IN_FLIGHT_REQUESTS: Declared<GaugeKind> =
    Declared::new("agentsfleet_api_in_flight_requests");

/// Streams refused at the cap.
pub const SSE_BACKPRESSURE_REJECTIONS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_sse_backpressure_rejections_total");

/// Approaching the stream cap.
pub const SSE_IN_FLIGHT_STREAMS: Declared<GaugeKind> =
    Declared::new("agentsfleet_sse_in_flight_streams");

/// Slow consumers losing frames.
pub const SSE_DROPPED_FRAMES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_sse_dropped_frames_total");

/// Pub/sub redials; spikes mean Redis instability.
pub const SSE_HUB_RECONNECTS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_sse_hub_reconnects_total");

/// 0 when the worker should be up.
pub const WORKER_RUNNING: Declared<GaugeKind> = Declared::new("agentsfleet_worker_running");

/// Span budget shedding; storms stay visible.
///
/// Labels: `reason`.
pub const HTTP_TRACE_SUPPRESSED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_http_trace_suppressed_total");

/// Exporter ring fill per signal.
///
/// Labels: `signal`.
pub const OTLP_QUEUE_DEPTH: Declared<GaugeKind> = Declared::new("agentsfleet_otlp_queue_depth");

/// Telemetry loss counted at the source.
///
/// Labels: `signal,reason`.
pub const OTLP_ENTRIES_DISCARDED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_otlp_entries_discarded_total");

/// Model attribution gaps (never faked).
///
/// Labels: `attribute,reason`.
pub const OTEL_ATTRIBUTE_OMITTED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_otel_attribute_omitted_total");
