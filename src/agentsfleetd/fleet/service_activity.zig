//! agentsfleetd-side runner control-plane orchestration — the `activity` verb.
//!
//! `POST /v1/runners/me/leases/{lease_id}/activity` forwards the live-tail
//! progress frames a runner streams off its sandboxed child. A runner holds no
//! Redis, so it ships frames here and `agentsfleetd` does the `PUBLISH` to
//! `zombie:{id}:activity` — downstream Server-Sent-Events (SSE) is unchanged.
//!
//! Best-effort + ephemeral by contract: a dropped frame is cosmetic (the durable
//! record is `report`), so each publish swallows its own failure and the verb
//! answers 202 with no ack. The only hard checks are authz — the lease must
//! resolve and belong to the presenting runner, else a runner could publish onto
//! a zombie it has no lease on. No fencing: a stale holder's cosmetic frames are
//! harmless, and the live tail is never the source of truth.
//!
//! This is the single seam where the runner-wire frame vocabulary
//! (`contract.activity`) is translated to the SSE/UI vocabulary
//! (`activity_publisher.KIND_*`, mirrored in `events.ts`).
//!
//! Allocator: per-request arena (`hx.alloc`).

const std = @import("std");
const logging = @import("log");
const httpz = @import("httpz");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const hx_mod = @import("../http/handlers/hx.zig");
const ec = @import("../errors/error_registry.zig");
const activity_wire = @import("contract").activity;
const activity_publisher = @import("../zombie/activity_publisher.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_activity);

/// The lease fields a publish needs: which zombie's channel, and the event the
/// frames belong to. Arena-dup'd off the row.
const Target = struct {
    zombie_id: []const u8,
    event_id: []const u8,
};

/// POST /v1/runners/me/leases/{lease_id}/activity — forward live-tail frames.
pub fn activity(hx: Hx, req: *httpz.Request, lease_id: []const u8) void {
    const runner_id = hx.principal.runner_id orelse {
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner identity required");
        return;
    };
    const raw_body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(activity_wire.ActivityRequest, hx.alloc, raw_body, .{}) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed activity body");
        return;
    };
    defer parsed.deinit();

    const target = loadTarget(hx, runner_id, lease_id) orelse {
        hx.fail(ec.ERR_RUN_LEASE_NOT_FOUND, "No lease matches this lease_id for the runner");
        return;
    };

    publishFrames(hx, target, parsed.value.frames);
    hx.ok(.accepted, Ack{ .ok = true });
}

const Ack = struct { ok: bool };

/// Publish every frame to the zombie's activity channel, reusing one Scratch
/// across the batch (the publisher's steady-state zero-alloc path).
fn publishFrames(hx: Hx, target: Target, frames: []const activity_wire.ActivityFrame) void {
    var scratch = activity_publisher.Scratch.init(hx.alloc);
    defer scratch.deinit();
    for (frames) |frame| publishOne(hx.ctx.queue, &scratch, hx.alloc, target, frame);
}

/// Translate one runner-wire frame to its SSE publish. This switch is the whole
/// vocabulary bridge: the compiler enforces a case per `ActivityFrame` variant.
fn publishOne(
    client: anytype,
    scratch: *activity_publisher.Scratch,
    alloc: std.mem.Allocator,
    target: Target,
    frame: activity_wire.ActivityFrame,
) void {
    switch (frame) {
        .tool_call_started => |b| activity_publisher.publishToolCallStarted(client, scratch, alloc, target.zombie_id, target.event_id, b.name, b.args_redacted),
        .agent_response_chunk => |b| activity_publisher.publishChunk(client, scratch, target.zombie_id, target.event_id, b.text),
        .tool_call_completed => |b| activity_publisher.publishToolCallCompleted(client, scratch, target.zombie_id, target.event_id, b.name, b.ms),
        .tool_call_progress => |b| activity_publisher.publishToolCallProgress(client, scratch, target.zombie_id, target.event_id, b.name, b.elapsed_ms),
    }
}

/// Resolve the lease scoped to the presenting runner. A foreign or stale
/// `lease_id` yields null → 404; the runner-id scope is the ownership check. No
/// status filter: an expired lease still resolves (cosmetic frames are harmless).
fn loadTarget(hx: Hx, runner_id: []const u8, lease_id: []const u8) ?Target {
    return loadTargetInner(hx, runner_id, lease_id) catch |err| {
        log.warn("activity_lease_load_failed", .{ .lease_id = lease_id, .err = @errorName(err) });
        return null;
    };
}

fn loadTargetInner(hx: Hx, runner_id: []const u8, lease_id: []const u8) !?Target {
    const conn = try hx.ctx.pool.acquire();
    defer hx.ctx.pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT zombie_id::text, event_id
        \\FROM fleet.runner_leases WHERE id = $1::uuid AND runner_id = $2::uuid
    , .{ lease_id, runner_id }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    return Target{
        .zombie_id = try hx.alloc.dupe(u8, try row.get([]const u8, 0)),
        .event_id = try hx.alloc.dupe(u8, try row.get([]const u8, 1)),
    };
}
