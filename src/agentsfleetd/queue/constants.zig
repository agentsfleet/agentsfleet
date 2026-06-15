const std = @import("std");
const constants_common = @import("common");

/// Stable prefix for `stableConsumerId` ("agentsfleetd-{host}"): one consumer per
/// agentsfleetd instance, timestamp-free, so Pending Entries List (PEL) entries
/// survive probes and restarts and group cardinality stays bounded.
pub const consumer_prefix = "agentsfleetd";

/// XAUTOCLAIM cursor seed + per-call batch size. Shared with the agent
/// stream XAUTOCLAIM in `redis_agent.zig`.
pub const xautoclaim_start = "0-0";
pub const xautoclaim_count = "1";

// ── Agent event stream constants ────────────────────────────────────────

/// Agent stream key format: "agent:{agent_id}:events".
/// Built dynamically per agent — not a single global stream.
pub const agent_stream_prefix = "agent:";
pub const agent_stream_suffix = ":events";

/// Consumer group for agent event processing. One group per agent stream.
/// Named for the lease path that reads it (agentsfleetd consumes on a runner's
/// behalf), not the retired worker process. Pre-launch rename from
/// "agent_workers": old groups carry no pending entries, so no drain is
/// needed — new streams create this group via ensureAgentConsumerGroup.
pub const agent_consumer_group = "agent_lease";

/// Stream field names for agent events. Wire shape matches EventEnvelope.encodeForXAdd.
/// The Redis stream entry id IS the canonical event_id — never carry a separate id.
pub const agent_field_type = "type";
pub const agent_field_actor = "actor";
pub const agent_field_workspace_id = "workspace_id";
pub const agent_field_request = "request";
pub const agent_field_created_at = "created_at";

/// XREADGROUP settings for agent streams.
pub const agent_xread_count = "1";

/// Reclaim min-idle: a PEL entry younger than this is never auto-claimed. The
/// per-agent affinity claim is the first belt against double-leasing; this
/// comptime relation is the second — the sweep can never race the lease
/// window of a just-delivered entry.
pub const agent_xautoclaim_min_idle_ms_int: i64 = 300_000;
comptime {
    if (agent_xautoclaim_min_idle_ms_int <= constants_common.LEASE_TTL_MS)
        @compileError("agent_xautoclaim_min_idle_ms_int must exceed LEASE_TTL_MS — reclaim must never race a live lease window");
}
pub const agent_xautoclaim_min_idle_ms = std.fmt.comptimePrint("{d}", .{agent_xautoclaim_min_idle_ms_int});

/// Background reclaim sweep cadence.
pub const agent_reclaim_interval_ms: i64 = 60_000;
