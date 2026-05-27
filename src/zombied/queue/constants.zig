/// Stable prefix for `makeConsumerId` ("worker-{host}-{ts}"). Used by the
/// zombie event-loop's per-thread consumer-id generation.
pub const consumer_prefix = "worker";

/// XAUTOCLAIM cursor seed + per-call batch size. Shared with the zombie
/// stream XAUTOCLAIM in `redis_zombie.zig`.
pub const xautoclaim_start = "0-0";
pub const xautoclaim_count = "1";

// ── Zombie event stream constants ────────────────────────────────────────

/// Zombie stream key format: "zombie:{zombie_id}:events".
/// Built dynamically per zombie — not a single global stream.
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
