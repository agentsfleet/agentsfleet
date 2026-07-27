//! Lease assignment — choose the next fleet + event for a polling runner.
//!
//! One pass per `lease` call (no server-side long-poll loop; the runner
//! re-polls via `retry_after_ms`). The pass is **ready-first**:
//!
//!   0. `fleet_ready.peek` — a bounded, randomized read of the shared readiness
//!      index, BEFORE any pool acquisition. An empty index answers no-work with
//!      zero Postgres round-trips, which is the dominant steady state on any
//!      deployment holding more fleets than concurrent events.
//!   1. The candidate query, restricted to those fleets and capped at
//!      `MAX_READY_CANDIDATES_PER_POLL`. Readiness narrows the input; it never
//!      decides eligibility — the label gate and sticky ordering are properties
//!      of the query and stay exactly where they were.
//!
//! Then, per candidate:
//!
//!   2. `affinity.claim` — the atomic per-fleet CLAIM. Exactly one of N racing
//!      runners wins the slot; a loser gets `.taken` and moves on, having read
//!      no event (claim precedes the read ⇒ nothing is orphaned).
//!   3. won + a prior `active` lease exists  → RECLAIM that dead holder's event
//!      from Postgres (no Redis re-read, no re-billing).
//!   4. won + no prior active lease           → FRESH: non-blocking XREADGROUP;
//!      no event ⇒ release the claim, clear readiness, try the next candidate.
//!
//! Every non-success exit after a win frees the claim — the no-work branches
//! inline, the post-claim error branches via `releaseWonClaim`. A transient
//! reclaim or envelope-allocation failure therefore costs one poll, not a full
//! `LEASE_TTL_MS` stall on that fleet.
//!
//! A run of failing Redis reads ends the candidate loop early rather than
//! holding the pooled connection through one timeout per remaining candidate —
//! see `PollCost.zig`.
//!
//! The result envelope is arena-dup'd into `select`'s `alloc` (the request arena
//! in production); the caller (service.zig) loads the session + bills (fresh) or
//! reuses billing (reclaim) + issues.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const handlers_common = @import("../http/handlers/common.zig");
const affinity = @import("affinity.zig");
const reclaim = @import("reclaim.zig");
const sql = @import("sql.zig");
const constants = @import("common");
const redis_fleet = @import("../queue/redis_fleet.zig");
const fleet_ready = @import("../queue/fleet_ready.zig");
const queue_redis = @import("../queue/redis_client.zig");
const fleet_config = @import("../fleet_runtime/config.zig");
const PollCost = @import("PollCost.zig");

const Context = handlers_common.Context;
const log = logging.scoped(.runner_assign);

pub const Kind = enum { fresh, reclaim };

/// Billing context carried forward on a reclaim (reused, never re-charged).
pub const Reused = struct {
    tenant_id: []const u8,
    posture: []const u8,
    model: []const u8,
};

/// The chosen work: the claimed fleet + fencing token + event envelope. For a
/// reclaim, `reused` carries the prior lease's billing; for fresh it is null
/// and the caller bills. All slices arena-dup'd.
pub const Acquired = struct {
    fleet_id: []const u8,
    fencing_token: u64,
    leased_until: i64,
    kind: Kind,
    event_id: []const u8,
    actor: []const u8,
    event_type: []const u8,
    request_json: []const u8,
    workspace_id: []const u8,
    event_created_at: i64,
    reused: ?Reused = null,
};

/// A ready fleet that survived the candidate query, carrying the readiness token
/// observed when it was peeked. The token travels to the clear site: readiness may
/// only be removed for this fleet if the stored token is still this one.
///
/// Null only if the peeked id and the query's `id::text` failed to match as text,
/// which canonical lowercase UUIDs make impossible — a null then costs a skipped
/// clear (the sweeper recovers) rather than an unsafe unconditional delete.
const Candidate = struct {
    fleet_id: []const u8,
    ready_token: ?[]const u8,
};

/// Select the next work for `runner_id`, or null when nothing is leasable this
/// pass. Errors are logged and collapse to null (the runner backs off + re-polls).
///
/// `alloc` is the request arena every returned slice is dup'd into. It is passed
/// explicitly rather than read off a handler context so a test can inject an
/// allocation failure and prove the won claim is released on that exit.
pub fn select(ctx: *Context, alloc: std.mem.Allocator, runner_id: []const u8) ?Acquired {
    var cost = PollCost{};
    defer cost.report();
    return selectInner(ctx, alloc, runner_id, &cost) catch |err| {
        log.warn("assign_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .runner_id = runner_id, .err = @errorName(err) });
        return null;
    };
}

fn selectInner(ctx: *Context, alloc: std.mem.Allocator, runner_id: []const u8, cost: *PollCost) !?Acquired {
    // A failed peek is retryable, never fatal, and never falls back to an
    // unbounded scan: answer no-work with the existing backoff hint and let the
    // runner re-poll (RULE ECL).
    const ready = fleet_ready.peek(ctx.queue, alloc, constants.MAX_READY_CANDIDATES_PER_POLL) catch |err| {
        log.warn("assign_ready_peek_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .runner_id = runner_id, .err = @errorName(err) });
        return null;
    };
    defer fleet_ready.freePeeked(alloc, ready);

    // The zero-Postgres path. Returning here — above `pool.acquire()` — is what
    // makes idle cost scale with runner count alone instead of runners × fleets.
    if (ready.len == 0) return null;

    const conn = try ctx.pool.acquire();
    defer ctx.pool.release(conn);
    const candidates = try listReadyCandidates(conn, alloc, runner_id, ready, cost);
    for (candidates) |candidate| {
        cost.candidates_examined += 1;
        if (try tryCandidate(ctx, conn, alloc, runner_id, candidate, cost)) |acq| return acq;
        // Stop rather than pin `conn` through one Redis timeout per remaining
        // candidate; no-work is what the exhausted loop answers anyway.
        if (cost.redisBrownedOut()) {
            log.warn("assign_redis_brownout_bailout", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .runner_id = runner_id, .candidates_examined = cost.candidates_examined });
            break;
        }
    }
    return null;
}

/// Run the bounded candidate query over the peeked fleets and re-attach each
/// survivor's readiness token, preserving the SQL's ordering.
///
/// The ordering must come from the query and not from the peek, because sticky
/// preference lives in its ORDER BY. The token lookup is linear over at most the
/// per-poll ceiling of entries, which is cheaper than building a map.
fn listReadyCandidates(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    runner_id: []const u8,
    ready: []const fleet_ready.Ready,
    cost: *PollCost,
) ![]Candidate {
    const ids = try alloc.alloc([]const u8, ready.len);
    for (ready, 0..) |entry, i| ids[i] = entry.fleet_id;

    cost.countDb(1);
    var q = PgQuery.from(try conn.query(sql.SELECT_READY_CANDIDATES, .{
        fleet_config.FleetStatus.active.toSlice(),
        runner_id,
        ids,
        @as(i64, @intCast(constants.MAX_READY_CANDIDATES_PER_POLL)),
    }));
    defer q.deinit();

    var out: std.ArrayList(Candidate) = .empty;
    while (try q.next()) |row| {
        const fleet_id = try alloc.dupe(u8, try row.get([]const u8, 0));
        try out.append(alloc, .{ .fleet_id = fleet_id, .ready_token = tokenFor(ready, fleet_id) });
    }
    return out.toOwnedSlice(alloc);
}

fn tokenFor(ready: []const fleet_ready.Ready, fleet_id: []const u8) ?[]const u8 {
    for (ready) |entry| {
        if (std.mem.eql(u8, entry.fleet_id, fleet_id)) return entry.token;
    }
    log.warn("assign_ready_token_unmatched", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id });
    return null;
}

/// Free the just-won slot after a post-claim failure so the fleet is claimable
/// on the next poll instead of stalling until the claim's own LEASE_TTL_MS
/// expiry. `release` is token-guarded and idempotent, and its own failure only
/// degrades to that expiry — so it is reported, never allowed to mask the error
/// the caller is about to re-raise.
fn releaseWonClaim(conn: *pg.Conn, fleet_id: []const u8, token: u64, stage: Kind, err: anyerror) void {
    const released = if (affinity.release(conn, fleet_id, token)) |_| true else |_| false;
    log.warn("post_claim_error_released", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id, .fencing_token = token, .stage = @tagName(stage), .released = released, .err = @errorName(err) });
}

/// Claim the fleet; on a win, reclaim a dead holder's event or take a fresh
/// one. Returns null when the slot is taken or has no leasable work.
fn tryCandidate(ctx: *Context, conn: *pg.Conn, alloc: std.mem.Allocator, runner_id: []const u8, candidate: Candidate, cost: *PollCost) !?Acquired {
    cost.countDb(1);
    const won = switch (try affinity.claim(conn, alloc, candidate.fleet_id, runner_id, constants.LEASE_TTL_MS)) {
        .taken => return null,
        .won => |w| w,
    };
    cost.countDb(1);
    const maybe_prior = reclaim.reclaimPriorActive(conn, alloc, candidate.fleet_id) catch |err| {
        cost.countDb(1);
        releaseWonClaim(conn, candidate.fleet_id, won.token, .reclaim, err);
        return err;
    };
    if (maybe_prior) |prior| {
        log.debug("lease_reclaimed", .{ .fleet_id = candidate.fleet_id, .event_id = prior.event_id, .lease_id = prior.lease_id, .fencing_token = won.token, .runner_id = runner_id });
        return fromReclaim(candidate.fleet_id, won, prior);
    }
    return acquireFresh(ctx, conn, alloc, candidate, won, cost);
}

/// Pull the next event for the claimed fleet: the stable consumer's own PEL
/// first (re-delivering pending-gate re-polls and sweep-recovered strands —
/// safe because this claim win proves no live lease exists), then a fresh
/// undelivered entry. No event ⇒ release the claim so the next event (and
/// other runners) are not blocked, and return null.
fn acquireFresh(ctx: *Context, conn: *pg.Conn, alloc: std.mem.Allocator, candidate: Candidate, won: affinity.Won, cost: *PollCost) !?Acquired {
    const fleet_id = candidate.fleet_id;
    // No group-ensure here. The group is created on the fleet's write path and is
    // durable, so asserting it per candidate per poll bought one Redis round-trip
    // apiece to re-learn something already true. If it is genuinely gone, the read
    // below answers `NOGROUP` and repairs itself — see `redis_fleet.readGroup`.
    var consumer_buf: [queue_redis.CONSUMER_ID_BUF_LEN]u8 = undefined;
    const consumer_id = queue_redis.stableConsumerId(&consumer_buf);
    // A failed PEL read cannot prove the PEL is empty, so it must NOT fall
    // through to the fresh read — promoting a new entry over a possibly-pending
    // gate re-poll would break own-PEL-first ordering exactly when Redis is
    // degraded. Release the claim and deliver nothing; the next poll retries
    // (consistent with the fresh-read error path below). Readiness is NOT
    // cleared on either error path, for the same reason: a failed read is not
    // evidence of an empty stream.
    var maybe_event = redis_fleet.xreadgroupFleetPending(ctx.queue, fleet_id, consumer_id) catch |err| {
        log.warn("assign_pel_read_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id, .err = @errorName(err) });
        cost.countDb(1);
        cost.noteRedisFailure();
        try affinity.release(conn, fleet_id, won.token);
        return null;
    };
    if (maybe_event) |ev| {
        log.debug("assign_pel_redelivered", .{ .fleet_id = fleet_id, .event_id = ev.event_id });
    } else {
        maybe_event = redis_fleet.xreadgroupFleetOnce(ctx.queue, fleet_id, consumer_id) catch |err| {
            log.warn("assign_xreadgroup_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id, .err = @errorName(err) });
            cost.countDb(1);
            cost.noteRedisFailure();
            try affinity.release(conn, fleet_id, won.token);
            return null;
        };
    }
    // Both arms above reached a verdict, so Redis answered — including the
    // no-event case, which is a successful read of an empty stream.
    cost.noteRedisReachable();
    var event = maybe_event orelse {
        clearReadiness(ctx, candidate);
        cost.countDb(1);
        try affinity.release(conn, fleet_id, won.token);
        return null;
    };
    defer event.deinit(ctx.queue.alloc);
    return fromFresh(alloc, fleet_id, won, &event) catch |err| {
        cost.countDb(1);
        releaseWonClaim(conn, fleet_id, won.token, .fresh, err);
        return err;
    };
}

/// The ONE place readiness is cleared, reached only where BOTH reads returned
/// null — which is the only evidence this code ever has that a fleet holds
/// nothing deliverable. Do not move this to a stream-emptiness check: ingress
/// trims at `MAXLEN ~ 10000`, so delivered entries persist and a stream is
/// essentially never empty, which would leave every fleet that ever received an
/// event permanently in the index.
///
/// Token-guarded, and that guard is load-bearing rather than defensive. The
/// affinity claim serializes runners against each other, but ingress takes no
/// claim at all: it can append and mark at any instant, including between the two
/// null reads above and this call. An unconditional delete would erase the mark
/// for that genuinely undelivered event, and nothing would rediscover it until a
/// sweep pass — turning a sub-second pickup into a multi-second one on a
/// completely healthy system.
fn clearReadiness(ctx: *Context, candidate: Candidate) void {
    const token = candidate.ready_token orelse return;
    fleet_ready.clear(ctx.queue, candidate.fleet_id, token);
}

fn fromFresh(alloc: std.mem.Allocator, fleet_id: []const u8, won: affinity.Won, event: *const redis_fleet.FleetEvent) !Acquired {
    return .{
        .fleet_id = fleet_id,
        .fencing_token = won.token,
        .leased_until = won.leased_until,
        .kind = .fresh,
        .event_id = try alloc.dupe(u8, event.event_id),
        .actor = try alloc.dupe(u8, event.actor),
        .event_type = try alloc.dupe(u8, event.event_type),
        .request_json = try alloc.dupe(u8, event.request_json),
        .workspace_id = try alloc.dupe(u8, event.workspace_id),
        .event_created_at = event.created_at_ms,
        .reused = null,
    };
}

fn fromReclaim(fleet_id: []const u8, won: affinity.Won, prior: reclaim.PriorLease) Acquired {
    return .{
        .fleet_id = fleet_id,
        .fencing_token = won.token,
        .leased_until = won.leased_until,
        .kind = .reclaim,
        .event_id = prior.event_id,
        .actor = prior.actor,
        .event_type = prior.event_type,
        .request_json = prior.request_json,
        .workspace_id = prior.workspace_id,
        .event_created_at = prior.event_created_at,
        .reused = .{ .tenant_id = prior.tenant_id, .posture = prior.posture, .model = prior.model },
    };
}
