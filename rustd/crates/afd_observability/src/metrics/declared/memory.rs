//! What a fleet remembered, and what this process is holding.
//!
//! The memory families count a fleet's own recall — what was captured, what a
//! hydration window carried, what a cap evicted — and beside them sit the
//! process-level readings: resident memory, and the bytes a sensitive request
//! or response had erased from it.

use crate::metrics::family::{CounterKind, Declared, GaugeKind};

/// Durable-memory write volume.
pub const MEMORY_ENTRIES_CAPTURED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_memory_entries_captured_total");

/// Memory writes failing.
pub const MEMORY_PUSH_FAILURES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_memory_push_failures_total");

/// Hydration window fill.
pub const MEMORY_HYDRATION_WINDOW_ENTRIES: Declared<GaugeKind> =
    Declared::new("agentsfleet_memory_hydration_window_entries");

/// Hydration overflow (entries).
pub const MEMORY_HYDRATION_DROPPED_ENTRIES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_memory_hydration_dropped_entries_total");

/// Hydration overflow (bytes).
pub const MEMORY_HYDRATION_DROPPED_BYTES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_memory_hydration_dropped_bytes_total");

/// Cap pressure on stored memory.
pub const MEMORY_CAP_EVICTIONS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_memory_cap_evictions_total");

/// Captures clipped at the push byte budget.
pub const MEMORY_CAPTURE_TRUNCATED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_memory_capture_truncated_total");

/// Captures lost to validation.
pub const MEMORY_CAPTURE_SKIPPED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_memory_capture_skipped_total");

/// Searches finding nothing.
pub const MEMORY_SEARCH_ZERO_HITS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_memory_search_zero_hits_total");

/// Process RSS.
pub const PROCESS_RESIDENT_MEMORY_BYTES: Declared<GaugeKind> =
    Declared::new("agentsfleet_process_resident_memory_bytes");

/// Plaintext-erasure proof; no labels by design.
pub const SENSITIVE_REQUEST_ERASED_BYTES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_sensitive_request_erased_bytes_total");

/// Plaintext-erasure proof; no labels by design.
pub const SENSITIVE_RESPONSE_ERASED_BYTES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_sensitive_response_erased_bytes_total");

/// Sensitive writes failing.
pub const SENSITIVE_RESPONSE_WRITE_FAILURES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_sensitive_response_write_failures_total");
