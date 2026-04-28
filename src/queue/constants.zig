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
const cancel_key_prefix = "run:cancel:";

/// M21_001 §1.2: Redis key for queued interrupt messages (v1 — no reader in v2).
const interrupt_key_prefix = "run:interrupt:";
const interrupt_ttl_seconds: u32 = 300;

// ── Zombie event stream constants ────────────────────────────────────────

/// Zombie stream key format: "zombie:{zombie_id}:events".
/// Built dynamically per zombie — not a single global stream like run_queue.
pub const zombie_stream_prefix = "zombie:";
pub const zombie_stream_suffix = ":events";

/// Consumer group for zombie event processing. One group per zombie stream.
pub const zombie_consumer_group = "zombie_workers";

/// Stream field names for zombie events. Wire shape matches EventEnvelope.encodeForXAdd.
/// The Redis stream entry id IS the canonical event_id — never carry a separate id.
pub const zombie_field_type = "type";
pub const zombie_field_actor = "actor";
pub const zombie_field_workspace_id = "workspace_id";
pub const zombie_field_request = "request";
pub const zombie_field_created_at = "created_at";

/// XREADGROUP settings for zombie streams.
pub const zombie_xread_count = "1";
pub const zombie_xread_block_ms = "5000";
pub const zombie_xautoclaim_min_idle_ms = "300000";
pub const zombie_reclaim_interval_ms: i64 = 60_000;
