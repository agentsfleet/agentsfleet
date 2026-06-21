const std = @import("std");
const constants_common = @import("common");

/// Stable prefix for `stableConsumerId` ("agentsfleetd-{host}"): one consumer per
/// agentsfleetd instance, timestamp-free, so Pending Entries List (PEL) entries
/// survive probes and restarts and group cardinality stays bounded.
pub const consumer_prefix = "agentsfleetd";

/// XAUTOCLAIM cursor seed + per-call batch size. Shared with the fleet
/// stream XAUTOCLAIM in `redis_fleet.zig`.
pub const xautoclaim_start = "0-0";
pub const xautoclaim_count = "1";

// ── Fleet event stream constants ────────────────────────────────────────

/// Fleet stream key format: "fleet:{fleet_id}:events".
/// Built dynamically per fleet — not a single global stream.
pub const fleet_stream_prefix = "fleet:";
pub const fleet_stream_suffix = ":events";

/// Consumer group for fleet event processing. One group per fleet stream.
/// Named for the lease path that reads it (agentsfleetd consumes on a runner's
/// behalf), not the retired worker process. Pre-launch rename from
/// "fleet_workers": old groups carry no pending entries, so no drain is
/// needed — new streams create this group via ensureFleetConsumerGroup.
pub const fleet_consumer_group = "fleet_lease";

/// Stream field names for fleet events. Wire shape matches EventEnvelope.encodeForXAdd.
/// The Redis stream entry id IS the canonical event_id — never carry a separate id.
pub const fleet_field_type = "type";
pub const fleet_field_actor = "actor";
pub const fleet_field_workspace_id = "workspace_id";
pub const fleet_field_request = "request";
pub const fleet_field_created_at = "created_at";

/// XREADGROUP settings for fleet streams.
pub const fleet_xread_count = "1";

/// Reclaim min-idle: a PEL entry younger than this is never auto-claimed. The
/// per-fleet affinity claim is the first belt against double-leasing; this
/// comptime relation is the second — the sweep can never race the lease
/// window of a just-delivered entry.
pub const fleet_xautoclaim_min_idle_ms_int: i64 = 300_000;
comptime {
    if (fleet_xautoclaim_min_idle_ms_int <= constants_common.LEASE_TTL_MS)
        @compileError("fleet_xautoclaim_min_idle_ms_int must exceed LEASE_TTL_MS — reclaim must never race a live lease window");
}
pub const fleet_xautoclaim_min_idle_ms = std.fmt.comptimePrint("{d}", .{fleet_xautoclaim_min_idle_ms_int});

/// Background reclaim sweep cadence.
pub const fleet_reclaim_interval_ms: i64 = 60_000;
