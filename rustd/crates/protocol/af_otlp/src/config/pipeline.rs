//! Explicit batching replaces hidden SDK timing defaults in deployment profiles.

use garde::Validate;
use serde::Deserialize;

/// Per-process telemetry queues and signal export cadences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct PipelineSettings {
    /// Seconds between metric collection and export.
    #[garde(range(min = 1, max = 86_400))]
    pub metrics_export_interval_seconds: u64,
    /// Maximum wait before exporting a partial span batch.
    #[garde(range(min = 1, max = 3600))]
    pub trace_export_interval_seconds: u64,
    /// Maximum wait before exporting a partial log batch.
    #[garde(range(min = 1, max = 3600))]
    pub log_export_interval_seconds: u64,
    /// Optional file override for each export request's timeout.
    #[garde(range(min = 1, max = 300))]
    pub export_timeout_seconds: Option<u64>,
    /// Maximum queued spans across this process.
    #[garde(range(min = 1, max = 65_536))]
    pub trace_queue_capacity: usize,
    /// Maximum spans in one export batch.
    #[garde(range(min = 1, max = self.trace_queue_capacity))]
    pub trace_batch_size: usize,
    /// Maximum queued log records across this process.
    #[garde(range(min = 1, max = 65_536))]
    pub log_queue_capacity: usize,
    /// Maximum log records in one export batch.
    #[garde(range(min = 1, max = self.log_queue_capacity))]
    pub log_batch_size: usize,
}

impl Default for PipelineSettings {
    fn default() -> Self {
        Self {
            metrics_export_interval_seconds: crate::COLLECT_INTERVAL.as_secs(),
            trace_export_interval_seconds: crate::SPAN_SEND_EVERY.as_secs(),
            log_export_interval_seconds: 1,
            export_timeout_seconds: None,
            trace_queue_capacity: crate::SPAN_QUEUE,
            trace_batch_size: crate::SPAN_BATCH,
            log_queue_capacity: 2048,
            log_batch_size: 512,
        }
    }
}
