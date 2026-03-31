pub const stream_name = "run_queue";
pub const consumer_group = "workers";
pub const consumer_prefix = "worker";

pub const field_run_id = "run_id";
pub const field_attempt = "attempt";
pub const field_workspace_id = "workspace_id";

pub const xread_count = "1";
pub const xread_block_ms = "5000";
pub const xautoclaim_min_idle_ms = "300000";
pub const xautoclaim_start = "0-0";
pub const xautoclaim_count = "1";
pub const reclaim_interval_ms: i64 = 60_000;

/// Redis key prefix for run cancellation signals (M17_001 §3.1).
/// Full key: cancel_key_prefix ++ run_id, TTL 1h.
pub const cancel_key_prefix = "run:cancel:";
