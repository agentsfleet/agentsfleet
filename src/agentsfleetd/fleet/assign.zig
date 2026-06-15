//! Lease assignment — choose the next agent + event for a polling runner.
//!
//! One pass per `lease` call (no server-side long-poll loop; the runner
//! re-polls via `retry_after_ms`). The scan lists active agents sticky-first
//! (the runner's `last_runner_id` matches sort to the front, but sticky is a
//! preference, never ownership), then for each candidate:
//!
//!   1. `affinity.claim` — the atomic per-agent CLAIM. Exactly one of N racing
//!      runners wins the slot; a loser gets `.taken` and moves on, having read
//!      no event (claim precedes the read ⇒ nothing is orphaned).
//!   2. won + a prior `active` lease exists  → RECLAIM that dead holder's event
//!      from Postgres (no Redis re-read, no re-billing).
//!   3. won + no prior active lease           → FRESH: non-blocking XREADGROUP;
//!      no event ⇒ release the claim and try the next candidate.
//!
//! The result envelope is arena-dup'd (`hx.alloc`); the caller (service.zig)
//! loads the session + bills (fresh) or reuses billing (reclaim) + issues.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const hx_mod = @import("../http/handlers/hx.zig");
const affinity = @import("affinity.zig");
const reclaim = @import("reclaim.zig");
const constants = @import("common");
const redis_agent = @import("../queue/redis_agent.zig");
const queue_redis = @import("../queue/redis_client.zig");
const agent_config = @import("../agent/config.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_assign);

pub const Kind = enum { fresh, reclaim };

/// Billing context carried forward on a reclaim (reused, never re-charged).
pub const Reused = struct {
    tenant_id: []const u8,
    posture: []const u8,
    model: []const u8,
};

/// The chosen work: the claimed agent + fencing token + event envelope. For a
/// reclaim, `reused` carries the prior lease's billing; for fresh it is null
/// and the caller bills. All slices arena-dup'd.
pub const Acquired = struct {
    agent_id: []const u8,
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

/// Select the next work for `runner_id`, or null when nothing is leasable this
/// pass. Errors are logged and collapse to null (the runner backs off + re-polls).
pub fn select(hx: Hx, runner_id: []const u8) ?Acquired {
    return selectInner(hx, runner_id) catch |err| {
        log.warn("assign_failed", .{ .runner_id = runner_id, .err = @errorName(err) });
        return null;
    };
}

fn selectInner(hx: Hx, runner_id: []const u8) !?Acquired {
    const conn = try hx.ctx.pool.acquire();
    defer hx.ctx.pool.release(conn);
    const candidates = try listCandidates(conn, hx.alloc, runner_id);
    for (candidates) |agent_id| {
        if (try tryCandidate(hx, conn, runner_id, agent_id)) |acq| return acq;
    }
    return null;
}

/// Eligible active agents, sticky-first. Eligibility is a single label gate:
/// a agent is a candidate for this runner only when its `required_tags` (TEXT[])
/// are a subset of the runner's advertised `labels` (`required_tags <@ labels`;
/// empty tags ⊆ any labels ⇒ any runner — today's behaviour). The runner's labels
/// (stored JSONB) are bound as a constant TEXT[] via the uncorrelated subquery,
/// so `<@` is a `column <@ constant` shape that the `required_tags` GIN index can
/// serve — not a column-to-column join (which no index serves). The match filters
/// the candidate set here; the per-agent slot claim (affinity.claim) is
/// unchanged. Within the eligible set the runner's own affinity sorts to the
/// front (DESC on the boolean), then oldest-created. Sticky is ordering only.
fn listCandidates(conn: *pg.Conn, alloc: std.mem.Allocator, runner_id: []const u8) ![][]const u8 {
    var q = PgQuery.from(try conn.query(
        \\SELECT z.id::text
        \\FROM core.agents z
        \\LEFT JOIN fleet.runner_affinity a ON a.agent_id = z.id
        \\WHERE z.status = $1
        \\  AND z.required_tags <@ (
        \\        SELECT COALESCE(array_agg(e), '{}'::text[])
        \\        FROM jsonb_array_elements_text(
        \\               (SELECT CASE WHEN jsonb_typeof(labels) = 'array'
        \\                            THEN labels ELSE '[]'::jsonb END
        \\                FROM fleet.runners WHERE id = $2::uuid)
        \\             ) AS e
        \\      )
        \\ORDER BY (a.last_runner_id = $2::uuid) DESC NULLS LAST, z.created_at ASC
    , .{ agent_config.AgentStatus.active.toSlice(), runner_id }));
    defer q.deinit();
    var ids: std.ArrayList([]const u8) = .empty;
    while (try q.next()) |row| {
        try ids.append(alloc, try alloc.dupe(u8, try row.get([]const u8, 0)));
    }
    return ids.toOwnedSlice(alloc);
}

/// Claim the agent; on a win, reclaim a dead holder's event or take a fresh
/// one. Returns null when the slot is taken or has no leasable work.
fn tryCandidate(hx: Hx, conn: *pg.Conn, runner_id: []const u8, agent_id: []const u8) !?Acquired {
    const won = switch (try affinity.claim(conn, hx.alloc, agent_id, runner_id, constants.LEASE_TTL_MS)) {
        .taken => return null,
        .won => |w| w,
    };
    if (try reclaim.reclaimPriorActive(conn, hx.alloc, agent_id)) |prior| {
        log.info("lease_reclaimed", .{ .agent_id = agent_id, .event_id = prior.event_id, .lease_id = prior.lease_id, .fencing_token = won.token, .runner_id = runner_id });
        return fromReclaim(agent_id, won, prior);
    }
    return acquireFresh(hx, conn, agent_id, won, runner_id);
}

/// Pull the next event for the claimed agent: the stable consumer's own PEL
/// first (re-delivering pending-gate re-polls and sweep-recovered strands —
/// safe because this claim win proves no live lease exists), then a fresh
/// undelivered entry. No event ⇒ release the claim so the next event (and
/// other runners) are not blocked, and return null.
fn acquireFresh(hx: Hx, conn: *pg.Conn, agent_id: []const u8, won: affinity.Won, runner_id: []const u8) !?Acquired {
    _ = runner_id;
    redis_agent.ensureAgentConsumerGroup(hx.ctx.queue, agent_id) catch |err| {
        log.warn("assign_group_ensure_failed", .{ .agent_id = agent_id, .err = @errorName(err) });
        try affinity.release(conn, agent_id, won.token);
        return null;
    };
    var consumer_buf: [queue_redis.CONSUMER_ID_BUF_LEN]u8 = undefined;
    const consumer_id = queue_redis.stableConsumerId(&consumer_buf);
    // A failed PEL read cannot prove the PEL is empty, so it must NOT fall
    // through to the fresh read — promoting a new entry over a possibly-pending
    // gate re-poll would break own-PEL-first ordering exactly when Redis is
    // degraded. Release the claim and deliver nothing; the next poll retries
    // (consistent with the fresh-read error path below).
    var maybe_event = redis_agent.xreadgroupAgentPending(hx.ctx.queue, agent_id, consumer_id) catch |err| {
        log.warn("assign_pel_read_failed", .{ .agent_id = agent_id, .err = @errorName(err) });
        try affinity.release(conn, agent_id, won.token);
        return null;
    };
    if (maybe_event) |ev| {
        log.debug("assign_pel_redelivered", .{ .agent_id = agent_id, .event_id = ev.event_id });
    } else {
        maybe_event = redis_agent.xreadgroupAgentOnce(hx.ctx.queue, agent_id, consumer_id) catch |err| {
            log.warn("assign_xreadgroup_failed", .{ .agent_id = agent_id, .err = @errorName(err) });
            try affinity.release(conn, agent_id, won.token);
            return null;
        };
    }
    var event = maybe_event orelse {
        try affinity.release(conn, agent_id, won.token);
        return null;
    };
    defer event.deinit(hx.ctx.queue.alloc);
    return try fromFresh(hx.alloc, agent_id, won, &event);
}

fn fromFresh(alloc: std.mem.Allocator, agent_id: []const u8, won: affinity.Won, event: *const redis_agent.AgentEvent) !Acquired {
    return Acquired{
        .agent_id = agent_id,
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

fn fromReclaim(agent_id: []const u8, won: affinity.Won, prior: reclaim.PriorLease) Acquired {
    return Acquired{
        .agent_id = agent_id,
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
