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

/// Scratch width every caller sizes its stream-key buffer to. Comfortably
/// exceeds prefix + a canonical 36-char UUID + suffix (49 bytes); the slack
/// absorbs the non-UUID fleet ids test fixtures use.
pub const fleet_stream_key_buf_len: usize = 128;

/// Build a fleet's event-stream key into `buf`. Single-sourced here rather than
/// per call site: the producer in `redis_fleet.zig` and every consumer must
/// agree byte-for-byte on this key, and the producer previously built it from
/// its own inline literal (RULE UFS).
pub fn fleetStreamKey(buf: []u8, fleet_id: []const u8) ![]const u8 {
    // discipline: ok — returns a borrowed view into `buf` (bufPrint), not owned
    // memory, so neither ownership phrase applies. Same shape as
    // `events/activity_channel.zig`'s channel formatter.
    return std.fmt.bufPrint(buf, "{s}{s}{s}", .{ fleet_stream_prefix, fleet_id, fleet_stream_suffix });
}

/// Readiness index: ONE global hash whose fields are the fleet ids currently
/// holding work, each valued by the generation token its last mark minted. A
/// lease poll reads this before touching Postgres, so an idle poll costs one
/// bounded Redis read and no database round-trip at all.
///
/// Global-under-`fleet:` mirrors the retired `fleet:control` key shape rather
/// than the per-fleet `fleet:{id}:…` streams — there is exactly one index for
/// the whole deployment, shared by every replica (`docs/architecture/
/// runner_fleet.md` §Redis topology). It is a hint, never the system of record:
/// the streams are, and `reclaim_sweeper` re-derives lost entries from them.
pub const ready_index_key = "fleet:ready";

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
