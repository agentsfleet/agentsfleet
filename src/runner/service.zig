//! zombied-side runner control-plane orchestration — the `lease` verb.
//!
//! `leaseNext` re-orchestrates the worker write path's pre-execution steps
//! (insert-received → resolve tenant/provider → balance gate → debit receive →
//! approval gate → debit stage) plus the `secrets_map`/context-budget
//! resolution lifted from `executeInSandbox`, then XREADGROUPs one event for
//! the runner's single assigned zombie and persists a `fleet.runner_leases`
//! row. It calls the existing leaf helpers rather than refactoring
//! `writepath.run`, so the direct path stays byte-identical at the cost of
//! deliberate orchestration duplication — the shared control-plane abstraction
//! is a follow-up workstream.
//!
//! Faithful, non-atomic: the debit fires here (pre-execution estimate, never
//! re-charged at report). The single-zombie skeleton ships `inline` secrets
//! only; real cross-zombie assignment, sticky routing, and fencing
//! verification are follow-ups.
//!
//! Allocator: handlers run inside the per-request arena (`hx.alloc`). Every
//! resolution output (claimed session, tenant id, resolved provider, parsed
//! secret bodies, lease id, envelope) is arena-scoped and reclaimed when the
//! request ends — that is why there is no explicit `deinit`/`freeResolved` of
//! those values here. The decoded stream event is the one exception: it is
//! owned by the Redis client's allocator and freed with `redis.alloc`.

const std = @import("std");
const logging = @import("log");

const hx_mod = @import("../http/handlers/hx.zig");
const common = @import("../http/handlers/common.zig");
const ec = @import("../errors/error_registry.zig");
const protocol = @import("protocol.zig");
const id_format = @import("../types/id_format.zig");

const event_loop = @import("../zombie/event_loop.zig");
const event_loop_secrets = @import("../zombie/event_loop_secrets.zig");
const event_loop_context_resolve = @import("../zombie/event_loop_context_resolve.zig");
const event_loop_gate = @import("../zombie/event_loop_gate.zig");
const rows = @import("../zombie/event_loop_writepath_rows.zig");
const metering = @import("../zombie/metering.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const queue_redis = @import("../queue/redis_client.zig");
const worker_zombie = @import("../cmd/worker_zombie.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const tenant_provider = @import("../state/tenant_provider.zig");
const balance_policy = @import("../config/balance_policy.zig");
const event_envelope = @import("../zombie/event_envelope.zig");
const execution_policy = @import("../executor/execution_policy.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_lease);

/// How long an issued lease stays valid before the event becomes reclaimable.
const LEASE_TTL_MS: i64 = 30_000;
/// Backoff hint when there is no work to lease (no 204; the verb is always 200).
const NO_WORK_RETRY_AFTER_MS: u32 = 1_000;
/// Consumer-id fallback when `makeConsumerId` cannot allocate; a fixed id is
/// acceptable for the single-zombie skeleton (mirrors the event loop's local id).
const RUNNER_CONSUMER_FALLBACK = "runner-local";

/// Tenant + provider resolution carried from `runBilling` into `issueLease`.
/// Both fields are arena-scoped (see the module note).
const Billed = struct {
    tenant_id: []u8,
    resolved: tenant_provider.ResolvedProvider,
};

/// POST /v1/runners/me/leases — claim the next event for the runner's one
/// assigned zombie, bill it, and hand back the work + resolved policy. Always
/// 200: a `LeasePayload` when there is work, else `lease=null` + a backoff hint.
pub fn leaseNext(hx: Hx) void {
    const runner_id = hx.principal.runner_id orelse {
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner identity required");
        return;
    };
    const pool = hx.ctx.pool;
    const redis = hx.ctx.queue;

    const zombie_id = pickZombieId(hx.alloc, pool) orelse return replyNoWork(hx);

    var session = event_loop.claimZombie(hx.alloc, zombie_id, pool) catch |err| {
        log.info("lease_claim_unavailable", .{ .zombie_id = zombie_id, .err = @errorName(err) });
        return replyNoWork(hx);
    };
    defer session.deinit(hx.alloc);

    redis_zombie.ensureZombieConsumerGroup(redis, zombie_id) catch |err| {
        log.warn("lease_group_ensure_failed", .{ .zombie_id = zombie_id, .err = @errorName(err) });
        return replyNoWork(hx);
    };
    const consumer_id = queue_redis.makeConsumerId(hx.alloc) catch RUNNER_CONSUMER_FALLBACK;
    var event = (redis_zombie.xreadgroupZombie(redis, zombie_id, consumer_id) catch |err| {
        log.warn("lease_xreadgroup_failed", .{ .zombie_id = zombie_id, .err = @errorName(err) });
        return replyNoWork(hx);
    }) orelse return replyNoWork(hx);
    defer event.deinit(redis.alloc);

    const billed = runBilling(hx, &session, &event) orelse return replyNoWork(hx);

    issueLease(hx, runner_id, &session, &event, billed) catch |err| {
        log.err("lease_issue_failed", .{ .zombie_id = zombie_id, .err = @errorName(err) });
        common.internalDbError(hx.res, hx.req_id);
    };
}

/// First active zombie (real cross-zombie assignment is a follow-up). The id
/// and its siblings are arena-owned, so the unused tail is reclaimed at request
/// end — no explicit free.
fn pickZombieId(alloc: std.mem.Allocator, pool: *@import("pg").Pool) ?[]const u8 {
    const ids = worker_zombie.listActiveZombieIds(pool, alloc) catch |err| {
        log.warn("lease_zombie_discovery_failed", .{ .err = @errorName(err) });
        return null;
    };
    if (ids.len == 0) return null;
    return ids[0];
}

/// Mirror `event_loop_writepath.run` steps 1–7 via the leaf helpers, then
/// resolve the provider so the caller can build the lease. Any blocked/failed
/// gate returns null → the caller answers no-work. The event is left un-acked
/// on a blocked gate (the direct path's `markBlocked`/dead-letter consumption
/// arrives with real cross-zombie assignment); the skeleton's seeded event
/// clears every gate.
fn runBilling(hx: Hx, session: *event_loop.ZombieSession, event: *const redis_zombie.ZombieEvent) ?Billed {
    const alloc = hx.alloc;
    const pool = hx.ctx.pool;

    rows.insertReceivedRow(alloc, pool, session, event) catch |err| {
        log.err("lease_received_insert_failed", .{ .zombie_id = session.zombie_id, .event_id = event.event_id, .err = @errorName(err) });
        return null;
    };

    const tr = resolveTenant(alloc, pool, session.workspace_id) orelse return null;
    const ctx = metering.PreflightContext{
        .workspace_id = session.workspace_id,
        .zombie_id = session.zombie_id,
        .event_id = event.event_id,
        .posture = tr.resolved.mode,
        .model = tr.resolved.model,
    };
    const policy = balance_policy.resolveFromEnv(alloc);

    if (!metering.balanceCoversEstimate(pool, alloc, tr.tenant_id, tr.resolved.mode, tr.resolved.model, policy)) {
        log.info("lease_balance_exhausted", .{ .zombie_id = session.zombie_id });
        return null;
    }
    if (metering.debitReceive(pool, alloc, tr.tenant_id, ctx, policy) != .deducted) return null;
    switch (event_loop_gate.checkApprovalGate(alloc, session, event, pool, hx.ctx.queue)) {
        .passed => {},
        else => {
            log.info("lease_gate_blocked", .{ .zombie_id = session.zombie_id, .event_id = event.event_id });
            return null;
        },
    }
    if (metering.debitStage(pool, alloc, tr.tenant_id, ctx, policy) != .deducted) return null;

    return Billed{ .tenant_id = tr.tenant_id, .resolved = tr.resolved };
}

/// One pooled connection resolves tenant id then active provider, mirroring
/// `event_loop_writepath_resolve.resolveTenantAndProvider`'s drain order.
fn resolveTenant(alloc: std.mem.Allocator, pool: *@import("pg").Pool, workspace_id: []const u8) ?struct { tenant_id: []u8, resolved: tenant_provider.ResolvedProvider } {
    const conn = pool.acquire() catch |err| {
        log.warn("lease_resolve_acquire_failed", .{ .err = @errorName(err) });
        return null;
    };
    defer pool.release(conn);
    const tenant_id = tenant_billing.resolveTenantFromWorkspace(conn, alloc, workspace_id) catch |err| {
        log.err("lease_tenant_lookup_failed", .{ .workspace_id = workspace_id, .err = @errorName(err) });
        return null;
    };
    const resolved = tenant_provider.resolveActiveProvider(alloc, conn, tenant_id) catch |err| {
        log.warn("lease_provider_resolve_failed", .{ .workspace_id = workspace_id, .err = @errorName(err) });
        return null;
    };
    return .{ .tenant_id = tenant_id, .resolved = resolved };
}

/// Build the lease payload + persist the `fleet.runner_leases` row, then 200.
fn issueLease(hx: Hx, runner_id: []const u8, session: *event_loop.ZombieSession, event: *const redis_zombie.ZombieEvent, billed: Billed) !void {
    const ev_type = event_envelope.EventType.fromSlice(event.event_type) orelse {
        log.warn("lease_unknown_event_type", .{ .zombie_id = session.zombie_id, .event_type = event.event_type });
        return replyNoWork(hx);
    };
    const envelope = event_envelope{
        .event_id = event.event_id,
        .zombie_id = session.zombie_id,
        .workspace_id = event.workspace_id,
        .actor = event.actor,
        .event_type = ev_type,
        .request_json = event.request_json,
        .created_at = event.created_at_ms,
    };

    const lease_id = try id_format.generateRunnerLeaseId(hx.alloc);
    // Monotonic enough for one sequential zombie; per-zombie monotonic
    // assignment + verification is a follow-up. Recorded, not enforced.
    const fencing_token: u64 = @intCast(@max(std.time.milliTimestamp(), 0));
    const lease_expires_at = std.time.milliTimestamp() + LEASE_TTL_MS;

    try insertLeaseRow(hx, runner_id, session, event, billed, lease_id, fencing_token, lease_expires_at);

    log.info("lease_issued", .{ .zombie_id = session.zombie_id, .event_id = event.event_id, .lease_id = lease_id, .runner_id = runner_id });
    hx.ok(.ok, protocol.LeaseResponse{ .lease = .{
        .lease_id = lease_id,
        .fencing_token = fencing_token,
        .lease_expires_at = lease_expires_at,
        .secret_delivery = .@"inline",
        .event = envelope,
        .policy = resolveExecutionPolicy(hx, session),
    } });
}

/// `secrets_map` (inline, parsed bodies) + context budget — the resolution
/// `executeInSandbox` does per execution, lifted onto the lease wire. Secret
/// bodies are arena-scoped and serialized synchronously by `hx.ok`; they are
/// never logged (Invariant: no `secrets_map` bytes in logs).
fn resolveExecutionPolicy(hx: Hx, session: *event_loop.ZombieSession) execution_policy.ExecutionPolicy {
    const alloc = hx.alloc;
    const budget = event_loop_context_resolve.resolveContextBudget(session.config.context, session.config.model);
    var secrets_map: ?std.json.Value = null;
    if (session.config.credentials.len > 0) {
        if (event_loop_secrets.resolveSecretsMap(alloc, hx.ctx.pool, session.workspace_id, session.config.credentials)) |resolved| {
            var obj = std.json.ObjectMap.init(alloc);
            for (resolved) |entry| {
                obj.put(entry.name, entry.parsed.value) catch |err| log.warn("lease_secret_put_failed", .{ .err = @errorName(err) });
            }
            secrets_map = .{ .object = obj };
        } else |err| {
            log.warn("lease_secrets_resolve_failed", .{ .zombie_id = session.zombie_id, .err = @errorName(err) });
        }
    }
    return .{ .secrets_map = secrets_map, .context = budget };
}

fn insertLeaseRow(
    hx: Hx,
    runner_id: []const u8,
    session: *event_loop.ZombieSession,
    event: *const redis_zombie.ZombieEvent,
    billed: Billed,
    lease_id: []const u8,
    fencing_token: u64,
    lease_expires_at: i64,
) !void {
    const conn = hx.ctx.pool.acquire() catch return error.DbError;
    defer hx.ctx.pool.release(conn);
    const now_ms = std.time.milliTimestamp();
    _ = conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, zombie_id, workspace_id, tenant_id, event_id,
        \\   posture, model, fencing_token, lease_expires_at, status,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6,
        \\        $7, $8, $9, $10, $11, $12, $12)
    , .{
        lease_id,
        runner_id,
        session.zombie_id,
        event.workspace_id,
        billed.tenant_id,
        event.event_id,
        billed.resolved.mode.label(),
        billed.resolved.model,
        @as(i64, @intCast(fencing_token)),
        lease_expires_at,
        protocol.RUNNER_LEASE_STATUS_ACTIVE,
        now_ms,
    }) catch return error.DbError;
}

fn replyNoWork(hx: Hx) void {
    hx.ok(.ok, protocol.LeaseResponse{ .lease = null, .retry_after_ms = NO_WORK_RETRY_AFTER_MS });
}
