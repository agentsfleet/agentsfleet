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
const S_NOGROUP = "NOGROUP";
/// XGROUP CREATE start-id for the WRITE path: the stream is brand-new and
/// empty, so "deliver from the beginning" and "deliver from now" coincide.
const GROUP_START_STREAM_BEGIN = "0";
/// XGROUP CREATE start-id for the REPAIR path: the stream's newest entry at
/// create time, so nothing already resident — almost all of it delivered and
/// XACKed under the vanished group — is handed out again.
const GROUP_START_STREAM_END = "$";
const S_GROUP_MISSING_REPAIRED = "fleet_consumer_group_missing_repaired";
const S_GROUP_REPAIR_FAILED = "fleet_consumer_group_repair_failed";

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
    // Mark BEFORE the id dupe: the append is already durable, so an OOM on the
    // dupe must not also cost the readiness hint — the caller fails the request
    // and the sender retries, but this event is in the stream either way and
    // only the mark makes it promptly leasable.
    fleet_ready.mark(client, envelope.fleet_id);
    const owned_id = try client.alloc.dupe(u8, id_str);
    log.debug("xadd_fleet_event", .{ .fleet_id = envelope.fleet_id, .event_id = owned_id, .actor = envelope.actor, .type = envelope.event_type.toSlice() });
    return owned_id;
}

fn xaddFailed(envelope: EventEnvelope) anyerror {
    log.err(S_XADD_FLEET_EVENT_FAILED, .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = envelope.fleet_id, .actor = envelope.actor });
    return error.RedisXaddFailed;
}

/// XGROUP CREATE on a fleet's event stream (MKSTREAM, idempotent), delivering
/// from the stream's beginning.
///
/// Called on the WRITE path — once, when a fleet's stream is created
/// (`handlers/fleets/create_stream.zig`, which retries and rolls the Postgres row
/// back if it never succeeds), where the stream is brand-new and there is no
/// history to re-deliver. It is deliberately NOT on the lease poll: re-asserting
/// a durable invariant on every candidate of every poll cost one Redis
/// round-trip apiece and used the `BUSYGROUP` error reply as its steady state,
/// which is what a per-process memo then existed to hide. A group that goes
/// missing announces itself as `NOGROUP` on the very next read, so the poll path
/// can stop guessing and be told — and its repair in `readGroup` below creates
/// at the stream's END, not here.
pub fn ensureFleetConsumerGroup(client: *redis_client.Client, fleet_id: []const u8) !void {
    return createFleetConsumerGroup(client, fleet_id, GROUP_START_STREAM_BEGIN);
}

fn createFleetConsumerGroup(client: *redis_client.Client, fleet_id: []const u8, start_id: []const u8) !void {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const stream_key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    var resp = try client.commandAllowError(&.{
        "XGROUP",                          "CREATE", stream_key,
        queue_consts.fleet_consumer_group, start_id, "MKSTREAM",
    });
    defer resp.deinit(client.alloc);
    switch (resp) {
        .simple => |v| if (!std.mem.eql(u8, v, S_OK)) return error.RedisGroupCreateFailed,
        .err => |msg| {
            if (std.mem.indexOf(u8, msg, S_BUSYGROUP) == null) return error.RedisGroupCreateFailed;
        },
        else => return error.RedisGroupCreateFailed,
    }
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
/// ## The group's absence is self-announcing, and this is where it is repaired
///
/// The consumer group is created once on the fleet's write path, so the steady
/// state here is a plain read with no setup command in front of it. But that
/// invariant can genuinely break — a group deleted out of band, a Redis restart
/// without persistence, a failover to an empty replica, or a fleet whose stream
/// predates the create-on-write path — and every one of those surfaces the same
/// way: Redis answers this read with `NOGROUP`. Nothing has to predict it.
///
/// So a `NOGROUP` reply recreates the group and READS AGAIN, exactly once. The
/// group is recreated at the stream's newest entry (`$`), not at `0`: the stream
/// retains up to its trim length (~10k) of entries that were already delivered
/// and XACKed under the vanished group, and a group recreated at `0` would hand
/// every one of them out again — historical agent runs re-executing with real
/// provider spend and real connector writes. Recreated at `$`, nothing
/// historical re-runs; the cost is that entries appended while the group was
/// missing are skipped rather than delivered. That loss is bounded by the
/// groupless window and repairable by re-submission; a re-executed run cannot
/// be un-spent. The write path keeps the beginning, where the stream is
/// brand-new and the two spellings coincide.
///
/// The caller's answer comes from a read that genuinely SUCCEEDED against the
/// repaired group — the two shortcuts are both wrong. Returning "no event"
/// WITHOUT the re-read would tell `fleet/assign.zig` the PEL is empty, which it
/// is explicit must never be inferred from a read that did not succeed.
/// Reporting an ERROR trips `PollCost.noteRedisFailure`, and a run of those
/// ends the candidate loop early (`assign_ready_faults_integration_test`) — so
/// one fleet whose group went missing would starve every remaining candidate on
/// every poll, turning a routine self-heal into a fleet-wide stall. A repair is
/// not a fault and must not be counted as one.
///
/// A SECOND `NOGROUP` is a real fault and propagates: the create reported success,
/// so the group existing is no longer something this code can be wrong about. That
/// also bounds the recursion at one retry.
///
/// Any other error reply propagates unrepaired — creating a group in response to,
/// say, a `WRONGTYPE` would be an infinite create loop.
fn readGroup(
    client: *redis_client.Client,
    fleet_id: []const u8,
    consumer_id: []const u8,
    read_id: []const u8,
) !?FleetEvent {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const stream_key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    switch (try readGroupOnce(client, stream_key, consumer_id, read_id)) {
        .read => |event| return event,
        .group_missing => {},
    }
    // Nothing is logged until the repair is CONFIRMED by the read below. Claiming
    // it here would emit a successful-repair signal on a create that failed or a
    // group that is somehow still missing, while leasing stays stalled — the one
    // state an operator most needs to see distinguished from a self-heal.
    try createFleetConsumerGroup(client, fleet_id, GROUP_START_STREAM_END);
    switch (try readGroupOnce(client, stream_key, consumer_id, read_id)) {
        .read => |event| {
            log.warn(S_GROUP_MISSING_REPAIRED, .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id });
            return event;
        },
        .group_missing => {
            log.err(S_GROUP_REPAIR_FAILED, .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id });
            return error.RedisXreadgroupFailed;
        },
    }
}

/// One `XREADGROUP`, distinguishing "the group is not there" from both an event
/// and an empty read — the three outcomes `readGroup` has to tell apart.
const GroupRead = union(enum) {
    read: ?FleetEvent,
    group_missing,
};

fn readGroupOnce(
    client: *redis_client.Client,
    stream_key: []const u8,
    consumer_id: []const u8,
    read_id: []const u8,
) !GroupRead {
    var resp = try client.commandAllowError(&.{
        REDIS_XREADGROUP_COMMAND,          REDIS_GROUP_ARG,
        queue_consts.fleet_consumer_group, consumer_id,
        S_COUNT,                           queue_consts.fleet_xread_count,
        REDIS_STREAMS_ARG,                 stream_key,
        read_id,
    });
    defer resp.deinit(client.alloc);
    if (resp == .err) {
        if (std.mem.indexOf(u8, resp.err, S_NOGROUP) == null) return error.RedisXreadgroupFailed;
        return .group_missing;
    }
    return .{ .read = try decode.decodeSingleFleetEvent(client.alloc, resp) };
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
/// The stream `DEL` propagates its error so the caller can log the orphan —
/// including a server-side error REPLY (a `READONLY` after a failover), which
/// `commandAllowError` hands back as a value rather than an error and which
/// would otherwise be indistinguishable from a successful purge. The
/// readiness clear is best-effort by signature and never fails the delete —
/// a stale field costs one wasted candidate check, and the deleted fleet's own
/// `status` filter keeps it from ever being leased.
///
/// Deleting the stream deletes the consumer groups on it, and nothing in this
/// process claims otherwise — the group's existence is no longer memoized
/// anywhere, so there is no in-process state here to contradict the delete.
pub fn purgeFleetRedisState(client: *redis_client.Client, fleet_id: []const u8) !void {
    // Mark FIRST: it is independent of the stream delete, and a transport failure
    // on the `try` below must not leave the fleet's field squatting in the shared
    // readiness sample — the fleet is gone from Postgres, so a surviving entry can
    // only ever be wrong.
    fleet_ready.forceClear(client, fleet_id);
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const stream_key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    var resp = try client.commandAllowError(&.{ S_DEL, stream_key });
    defer resp.deinit(client.alloc);
    if (resp == .err) return error.RedisStreamPurgeFailed;
}
