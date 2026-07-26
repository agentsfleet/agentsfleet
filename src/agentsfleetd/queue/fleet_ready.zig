//! Readiness index — which fleets currently hold work.
//!
//! ONE global Redis hash (`constants.zig` owns the key): field = fleet id, value
//! = the generation token that fleet's last mark minted. Ingress marks a fleet
//! here right after appending to its stream; a lease poll reads this BEFORE it
//! opens a Postgres connection, so an idle poll costs one bounded Redis read and
//! no database round-trip at all. That is the whole point of the module.
//!
//! **The index is a hint, never the system of record** — the streams are. A lost
//! mark costs delivery latency, never the event: `fleet/reclaim_sweeper.zig`
//! re-derives readiness from the streams on its existing pass, so every write
//! here is best-effort and no failure may propagate into an accepted ingress
//! call or a lease reply.
//!
//! **Why fields carry a token.** A poll that finds a fleet holds nothing
//! deliverable clears it from the index. Ingress takes no per-fleet claim, so it
//! can append and mark at any instant — including inside the gap between that
//! poll's last read and its clear. An unconditional delete would erase a mark
//! for genuinely undelivered work, and nothing would rediscover it until a sweep
//! pass. `clear` therefore deletes a field only when its stored token still
//! equals the one the caller observed; an interleaved mark has replaced it, so
//! the clear finds a different token and no-ops.
//!
//! **Why the token is a minted identifier and not a counter.** Nothing here ever
//! compares two tokens for order — only for equality. Every counter shape fails
//! on reuse instead: `clear` deletes the field, so a per-fleet count restarts
//! and re-mints a token a stale poll still holds, and hoisting the counter into
//! a shared Redis key only moves the problem, since that key is itself
//! evictable. A fresh UUIDv7 per mark carries uniqueness in its entropy, needs
//! no second key, and depends on the clock for nothing but the chronological
//! sort order an operator reading the index benefits from.

const std = @import("std");
const logging = @import("log");
const queue_consts = @import("constants.zig");
const redis_client = @import("redis_client.zig");
const redis_protocol = @import("redis_protocol.zig");
const id_format = @import("../types/id_format.zig");
const metrics = @import("../observability/metrics_counters.zig");
const ec = @import("../errors/error_registry.zig");

const log = logging.scoped(.fleet_ready);

const CMD_HSET = "HSET";
const CMD_HDEL = "HDEL";
const CMD_HLEN = "HLEN";
const CMD_HRANDFIELD = "HRANDFIELD";
const CMD_EVAL = "EVAL";
const ARG_WITHVALUES = "WITHVALUES";
/// `EVAL <script> <numkeys>` — every script here addresses the one index key.
const EVAL_ONE_KEY = "1";
/// Widest decimal a `usize` peek bound can print.
const COUNT_DIGITS: usize = 20;

/// Delete the field only when it still carries the token the caller observed.
/// A caller-side read-then-delete does NOT express this: the gap between that
/// read and that delete is exactly the window a concurrent ingress mark wins.
/// Evaluating both halves inside Redis closes it, because a script body runs to
/// completion against a single-threaded server.
const CLEAR_IF_TOKEN_MATCHES =
    \\if redis.call('HGET', KEYS[1], ARGV[1]) == ARGV[2] then
    \\  return redis.call('HDEL', KEYS[1], ARGV[1])
    \\end
    \\return 0
;

/// One ready fleet plus the token naming this generation of its mark. Both
/// slices are owned by the allocator `peek` received; release via `freePeeked`,
/// passing that same allocator.
pub const Ready = struct {
    fleet_id: []const u8,
    token: []const u8,
};

/// Release a slice returned by `peek`, using the allocator it was given.
pub fn freePeeked(alloc: std.mem.Allocator, entries: []const Ready) void {
    freeItems(alloc, entries);
    alloc.free(entries);
}

/// Frees the strings owned by each entry but NOT the backing array.
///
/// Split out because the partial-failure path inside `decodePeek` must free the
/// entries built so far while freeing the array exactly once, at its true length.
/// Handing a sub-slice to `freePeeked` would call `alloc.free` on a view rather
/// than on the original allocation, which leaks it — the allocation-failure test
/// is what makes that mistake fail loudly instead of silently.
fn freeItems(alloc: std.mem.Allocator, entries: []const Ready) void {
    for (entries) |entry| {
        alloc.free(entry.fleet_id);
        alloc.free(entry.token);
    }
}

/// Record that `fleet_id` holds work. Call only AFTER the append succeeded, so a
/// failed append can never leave a falsely-ready fleet.
///
/// Best-effort and infallible by signature: the caller has already durably
/// accepted the event, and failing its request over a lost hint would trade a
/// latency cost for a correctness one. One `HSET` — the minted token needs no
/// server-side draw, so no script is involved.
pub fn mark(client: *redis_client.Client, fleet_id: []const u8) void {
    const token = id_format.generateUuidV7() catch |err| return writeFailed(WRITE_MARK, fleet_id, err);
    var resp = client.command(&.{
        CMD_HSET, queue_consts.ready_index_key, fleet_id, &token,
    }) catch |err| return writeFailed(WRITE_MARK, fleet_id, err);
    resp.deinit(client.alloc);
}

/// Remove `fleet_id`, but only if its stored token still equals `token` — the
/// value this caller observed when it established the fleet held nothing
/// deliverable. Taking the token as a required argument is what makes an
/// unconditional delete inexpressible at this call site.
///
/// Best-effort: a failed clear leaves a stale field, costing one wasted
/// candidate check on a later poll. That is strictly the safe direction — a
/// false positive costs a check, a false negative strands an event.
pub fn clear(client: *redis_client.Client, fleet_id: []const u8, token: []const u8) void {
    var resp = client.command(&.{
        CMD_EVAL,                     CLEAR_IF_TOKEN_MATCHES, EVAL_ONE_KEY,
        queue_consts.ready_index_key, fleet_id,               token,
    }) catch |err| return writeFailed(WRITE_CLEAR, fleet_id, err);
    resp.deinit(client.alloc);
}

/// Remove `fleet_id` unconditionally, with no token compare.
///
/// The token guard on `clear` exists to protect LIVE work: a racing ingress mark
/// means the fleet still holds a deliverable event, so that clear must no-op.
/// None of that reasoning survives a fleet leaving `active` — deleted, stopped,
/// killed, or paused, no runner can lease it, so there is no mark worth keeping
/// and a racing ingress mark is itself already stale.
///
/// Call this ONLY from a fleet-lifecycle transition that has already committed
/// in Postgres, never before it: an unconditional delete ahead of a rolled-back
/// transaction would erase a live fleet's mark. The poll path must keep using
/// `clear` — there an unconditional delete strands an event (§3), which is the
/// race this module was built to close.
///
/// Without this, a lifecycle transition leaves a field nothing can ever remove:
/// the clear at the poll site is reachable only for fleets the candidate query
/// returns, and that query filters `status = 'active'`.
///
/// Best-effort like every write here — a failure leaves a stale field, costing
/// one wasted candidate check on a later poll.
pub fn forceClear(client: *redis_client.Client, fleet_id: []const u8) void {
    var resp = client.command(&.{
        CMD_HDEL, queue_consts.ready_index_key, fleet_id,
    }) catch |err| return writeFailed(WRITE_CLEAR, fleet_id, err);
    resp.deinit(client.alloc);
}

/// Which write failed rides the LOG, not the metric. An operator responds the
/// same way to either (Redis is unreachable), so splitting the counter would add
/// a label nobody queries on.
const WRITE_MARK = "mark";
const WRITE_CLEAR = "clear";

fn writeFailed(which: []const u8, fleet_id: []const u8, err: anyerror) void {
    metrics.incReadyWriteFailure();
    log.warn("ready_write_failed", .{
        .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
        .write = which,
        .fleet_id = fleet_id,
        .err = @errorName(err),
    });
}

/// At most `max` ready fleets, sampled RANDOMLY rather than from the head.
///
/// Randomization is the anti-starvation property: a deterministic slice would
/// pin discovery to the same fleets on every poll and never reach the rest.
/// Reading the whole hash and slicing client-side would hand the per-fleet cost
/// straight back to the caller, which is the cost this index exists to remove —
/// so the bound is applied by the server, in the command.
///
/// Errors are retryable, never fatal: the caller answers no-work with its
/// existing backoff hint and never falls back to an unbounded scan.
pub fn peek(client: *redis_client.Client, alloc: std.mem.Allocator, max: usize) ![]Ready {
    if (max == 0) return &.{};
    var count_buf: [COUNT_DIGITS]u8 = undefined;
    const count = try std.fmt.bufPrint(&count_buf, "{d}", .{max});
    var resp = try client.command(&.{
        CMD_HRANDFIELD, queue_consts.ready_index_key, count, ARG_WITHVALUES,
    });
    defer resp.deinit(client.alloc);
    return decodePeek(alloc, resp);
}

/// RESP2 renders `HRANDFIELD … WITHVALUES` as ONE flat array alternating field
/// and value. This client sends no `HELLO`, so it never negotiates RESP3 and the
/// nested pair-array shape cannot appear here — decoding pairs by stepping two
/// is correct rather than merely convenient.
///
/// `pub` for `fleet_ready_test.zig`: this is the one piece of the module that is
/// a pure function of a reply, so it is where the wire-shape assumption and the
/// allocation-failure unwinding can be proven without a live datastore.
pub fn decodePeek(alloc: std.mem.Allocator, value: redis_protocol.RespValue) ![]Ready {
    if (value != .array) return &.{};
    const flat = value.array orelse return &.{};
    if (flat.len == 0) return &.{};
    if (flat.len % 2 != 0) return error.RedisUnexpectedResponse;

    const entries = try alloc.alloc(Ready, flat.len / 2);
    var filled: usize = 0;
    // Frees only what is already owned, so a dupe failure mid-array leaks
    // nothing — Zig does not unwind earlier iterations for us.
    errdefer {
        freeItems(alloc, entries[0..filled]);
        alloc.free(entries);
    }

    var i: usize = 0;
    while (i < flat.len) : (i += 2) {
        const field = redis_protocol.valueAsString(flat[i]) orelse return error.RedisUnexpectedResponse;
        const token = redis_protocol.valueAsString(flat[i + 1]) orelse return error.RedisUnexpectedResponse;
        const owned_id = try alloc.dupe(u8, field);
        errdefer alloc.free(owned_id);
        entries[filled] = .{ .fleet_id = owned_id, .token = try alloc.dupe(u8, token) };
        filled += 1;
    }
    return entries;
}

/// Field count of the shared index. Read by the reclaim sweeper once per pass
/// and handed to the metrics registry as a sample; `/metrics` renders that
/// sample from memory and never calls this, so the scrape path stays free of
/// both datastores.
pub fn depth(client: *redis_client.Client) !u64 {
    var resp = try client.command(&.{ CMD_HLEN, queue_consts.ready_index_key });
    defer resp.deinit(client.alloc);
    return switch (resp) {
        .integer => |n| if (n > 0) @intCast(n) else 0,
        else => error.RedisUnexpectedResponse,
    };
}

test {
    _ = @import("fleet_ready_test.zig");
}
