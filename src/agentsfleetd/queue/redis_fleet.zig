//! Fleet Redis stream commands.
//!
//! Operates on per-fleet streams (`fleet:{fleet_id}:events`) using XADD /
//! XREADGROUP / XAUTOCLAIM / XACK with the shared `fleet_lease` consumer group.
//! This file owns the command shapes; `redis_fleet_decode.zig` turns their
//! replies into owned events and `redis_fleet_probe.zig` answers "does this
//! fleet hold anything deliverable" for the reclaim sweeper.
//!
//! `xaddFleetEvent` lives here rather than on the `redis_client.Client` façade
//! because it is a fleet-stream operation, not generic pooled plumbing — and
//! because readiness marking belongs inside the one producer every ingress path
//! funnels through. Keeping it on the façade would make that façade depend on a
//! fleet-domain module in the other direction.

const std = @import("std");
const logging = @import("log");
const queue_consts = @import("constants.zig");
const redis_client = @import("redis_client.zig");
const decode = @import("redis_fleet_decode.zig");
const fleet_ready = @import("fleet_ready.zig");
const group_memo = @import("fleet_group_memo.zig");
const error_codes = @import("../errors/error_registry.zig");
const EventEnvelope = @import("contract").event_envelope;

const log = logging.scoped(.redis_fleet);

/// Re-exported so callers keep naming it `redis_fleet.FleetEvent` while the
/// decoders live in their own file and never import this one.
pub const FleetEvent = decode.FleetEvent;

const REDIS_GROUP_ARG = "GROUP";
const REDIS_STREAMS_ARG = "STREAMS";
const REDIS_XREADGROUP_COMMAND = "XREADGROUP";
const S_XACK_FAILED = "xack_failed";
const S_DEL = "DEL";
const S_XADD_FLEET_EVENT_FAILED = "xadd_fleet_event_failed";
const S_COUNT = "COUNT";
/// XREADGROUP id reading the consumer's own PEL from the start ("0") instead
/// of new entries (">").
const PEL_READ_ID = "0";
const NEW_ENTRIES_ID = ">";
const S_OK = "OK";
const S_BUSYGROUP = "BUSYGROUP";

// XADD argv slots for `xaddFleetEvent`. The `MAXLEN ~ 10000` triplet caps the
// stream's retention (~10k approximate trim); `*` asks Redis to generate the
// stream entry id, which IS the event_id.
const XADD_VERB = "XADD";
const XADD_MAXLEN_KEYWORD = "MAXLEN";
const XADD_MAXLEN_APPROX = "~";
const XADD_MAXLEN_FLEET_EVENTS = "10000";
const XADD_AUTO_ID = "*";

/// Compile-folded tail for `XADD fleet:{id}:events MAXLEN ~ 10000 * …`.
/// Slot 0 = `XADD`, slot 1 = stream key (runtime), slots 2..6 = this slice.
const XADD_TRIM_TAIL: []const []const u8 = &.{
    XADD_MAXLEN_KEYWORD,
    XADD_MAXLEN_APPROX,
    XADD_MAXLEN_FLEET_EVENTS,
    XADD_AUTO_ID,
};
const XADD_PREFIX_LEN: usize = 2 + XADD_TRIM_TAIL.len;

/// XADD an EventEnvelope onto `fleet:{envelope.fleet_id}:events`, then record the
/// fleet as ready. The Redis stream entry id IS the canonical event_id; this
/// returns it allocated via `client.alloc` so the caller can surface it for
/// stream correlation. Caller must free the returned entry id.
///
/// This is the ONE producer all five ingress paths reach (chat messages, both
/// webhook surfaces, Slack events, GitHub ingress), which is why readiness is
/// recorded here — once, rather than at five handlers that would drift. The mark
/// runs only after the append succeeds, so a failed append never leaves a
/// falsely-ready fleet, and it cannot fail this call (see `fleet_ready.mark`).
pub fn xaddFleetEvent(client: *redis_client.Client, envelope: EventEnvelope) ![]u8 {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const stream_key = try queue_consts.fleetStreamKey(&key_buf, envelope.fleet_id);

    const payload_argv = try envelope.encodeForXAdd(client.alloc);
    defer EventEnvelope.freeXAddArgv(client.alloc, payload_argv);

    const argv = try client.alloc.alloc([]const u8, XADD_PREFIX_LEN + payload_argv.len);
    defer client.alloc.free(argv);
    argv[0] = XADD_VERB;
    argv[1] = stream_key;
    @memcpy(argv[2..XADD_PREFIX_LEN], XADD_TRIM_TAIL);
    @memcpy(argv[XADD_PREFIX_LEN..], payload_argv);

    var resp = try client.command(argv);
    defer resp.deinit(client.alloc);

    const id_str = switch (resp) {
        .bulk => |v| v orelse return xaddFailed(envelope),
        else => return xaddFailed(envelope),
    };
    const owned_id = try client.alloc.dupe(u8, id_str);
    fleet_ready.mark(client, envelope.fleet_id);
    log.debug("xadd_fleet_event", .{ .fleet_id = envelope.fleet_id, .event_id = owned_id, .actor = envelope.actor, .type = envelope.event_type.toSlice() });
    return owned_id;
}

fn xaddFailed(envelope: EventEnvelope) anyerror {
    log.err(S_XADD_FLEET_EVENT_FAILED, .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = envelope.fleet_id, .actor = envelope.actor });
    return error.RedisXaddFailed;
}

/// XGROUP CREATE on a fleet's event stream (MKSTREAM, idempotent).
///
/// Memoized per process: the group is durable, so once created it stays created,
/// and the unmemoized form cost one Redis round-trip per candidate per poll
/// forever — relying on the `BUSYGROUP` error as its steady state. A cold
/// process, a memo overflow, or a genuinely new fleet still takes the real path.
pub fn ensureFleetConsumerGroup(client: *redis_client.Client, fleet_id: []const u8) !void {
    if (group_memo.isEnsured(fleet_id)) return;
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const stream_key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    var resp = try client.commandAllowError(&.{
        "XGROUP",                          "CREATE", stream_key,
        queue_consts.fleet_consumer_group, "0",      "MKSTREAM",
    });
    defer resp.deinit(client.alloc);
    switch (resp) {
        .simple => |v| if (!std.mem.eql(u8, v, S_OK)) return error.RedisGroupCreateFailed,
        .err => |msg| {
            if (std.mem.indexOf(u8, msg, S_BUSYGROUP) == null) return error.RedisGroupCreateFailed;
        },
        else => return error.RedisGroupCreateFailed,
    }
    group_memo.recordEnsured(fleet_id);
}

/// XREADGROUP on fleet:{id}:events reading the consumer's OWN Pending Entries
/// List (id "0") — re-delivers the oldest entry that was delivered but never
/// XACKed (a pending-gate re-poll, a sweep-recovered strand) before any new
/// event is read. Null when the PEL is empty. Safe against double-leasing: the
/// caller holds the fleet's affinity claim, so no other holder can be mid-run on
/// this entry.
pub fn xreadgroupFleetPending(
    client: *redis_client.Client,
    fleet_id: []const u8,
    consumer_id: []const u8,
) !?FleetEvent {
    return readGroup(client, fleet_id, consumer_id, PEL_READ_ID);
}

/// XREADGROUP on fleet:{id}:events WITHOUT BLOCK — returns the next undelivered
/// event immediately, or null. The assignment scan probes several fleets per poll
/// and must not park on any single stream; the runner long-polls client-side (via
/// `retry_after_ms`) instead.
pub fn xreadgroupFleetOnce(
    client: *redis_client.Client,
    fleet_id: []const u8,
    consumer_id: []const u8,
) !?FleetEvent {
    return readGroup(client, fleet_id, consumer_id, NEW_ENTRIES_ID);
}

/// Shared body of the two group reads — they differ only in the read id.
///
/// A read failure invalidates the group memo. A group deleted out-of-band
/// surfaces as a read error, and without dropping the memo entry this fleet would
/// keep skipping XGROUP CREATE and keep failing until process restart.
/// Invalidating on any read error rather than on a parsed `NOGROUP` message costs
/// at most one redundant create on the next poll.
fn readGroup(
    client: *redis_client.Client,
    fleet_id: []const u8,
    consumer_id: []const u8,
    read_id: []const u8,
) !?FleetEvent {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const stream_key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    var resp = client.command(&.{
        REDIS_XREADGROUP_COMMAND,          REDIS_GROUP_ARG,
        queue_consts.fleet_consumer_group, consumer_id,
        S_COUNT,                           queue_consts.fleet_xread_count,
        REDIS_STREAMS_ARG,                 stream_key,
        read_id,
    }) catch |err| {
        group_memo.invalidate(fleet_id);
        return err;
    };
    defer resp.deinit(client.alloc);
    return decode.decodeSingleFleetEvent(client.alloc, resp);
}

/// XAUTOCLAIM one stale fleet event (idle past the comptime-bounded min-idle)
/// into `consumer_id`'s PEL — the recovery half for entries orphaned under a
/// dead consumer name (retired instance, legacy throwaway ids). The claimed
/// entry is then re-delivered by `xreadgroupFleetPending` on the next lease poll.
pub fn xautoclaimFleet(
    client: *redis_client.Client,
    fleet_id: []const u8,
    consumer_id: []const u8,
) !?FleetEvent {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const stream_key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    var resp = try client.command(&.{
        "XAUTOCLAIM",                              stream_key,
        queue_consts.fleet_consumer_group,         consumer_id,
        queue_consts.fleet_xautoclaim_min_idle_ms, queue_consts.xautoclaim_start,
        S_COUNT,                                   queue_consts.xautoclaim_count,
    });
    defer resp.deinit(client.alloc);
    return decode.decodeAutoClaimFleetEvent(client.alloc, resp);
}

/// XACK on a fleet event stream after successful processing.
pub fn xackFleet(client: *redis_client.Client, fleet_id: []const u8, event_id: []const u8) !void {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const stream_key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    var resp = try client.command(&.{
        "XACK",                            stream_key,
        queue_consts.fleet_consumer_group, event_id,
    });
    defer resp.deinit(client.alloc);
    switch (resp) {
        .integer => |v| if (v < 0) return xackFailed(fleet_id, event_id),
        else => return xackFailed(fleet_id, event_id),
    }
    log.debug("xack_succeeded", .{ .fleet_id = fleet_id, .event_id = event_id });
}

fn xackFailed(fleet_id: []const u8, event_id: []const u8) anyerror {
    log.err(S_XACK_FAILED, .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id, .event_id = event_id });
    return error.RedisXackFailed;
}

/// Drop every Redis trace of a hard-deleted fleet: its event stream and its
/// readiness mark. Owned here rather than at the HTTP call site because this
/// module already owns the stream key and every other fleet-stream command —
/// the handler's private helper duplicated the key literal instead (RULE UFS).
///
/// Call only AFTER the Postgres purge commits. Ordering is the whole safety
/// argument: with the row gone the fleet can never be a lease candidate again,
/// so anything left here is inert. Run before the commit, and a rolled-back
/// delete would have erased a live fleet's stream.
///
/// The stream `DEL` propagates its error so the caller can log the orphan; the
/// readiness clear is best-effort by signature and never fails the delete —
/// a stale field costs one wasted candidate check, and the deleted fleet's own
/// `status` filter keeps it from ever being leased.
///
/// The group memo is dropped here too, because deleting a stream deletes the
/// consumer groups on it. The memo is an in-process claim about exactly the
/// state this call destroys, so leaving it would have the cache contradicting
/// its own store: the next `ensureFleetConsumerGroup` short-circuits on a group
/// that is gone, and the read behind it spends a whole poll discovering
/// `NOGROUP` before the read-error path invalidates the entry. That recovery is
/// the backstop for an out-of-band delete, not a licence to hand it a stale
/// entry we created ourselves.
pub fn purgeFleetRedisState(client: *redis_client.Client, fleet_id: []const u8) !void {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const stream_key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    var resp = try client.commandAllowError(&.{ S_DEL, stream_key });
    resp.deinit(client.alloc);
    group_memo.invalidate(fleet_id);
    fleet_ready.forceClear(client, fleet_id);
}
