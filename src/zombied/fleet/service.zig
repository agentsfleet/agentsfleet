//! zombied-side runner control-plane orchestration — the `lease` verb.
//!
//! `leaseNext` delegates assignment to `assign.select`: across all active
//! zombies it atomically CLAIMS one (sticky-preferred), then either reclaims an
//! expired holder's event or pulls a fresh one. For a fresh lease it
//! re-orchestrates the worker write path's pre-execution steps (insert-received
//! → resolve tenant/provider → balance gate → debit receive → approval gate →
//! debit stage) plus the `secrets_map`/context-budget resolution lifted from
//! `executeInSandbox`; a reclaim reuses the prior lease's billing (no
//! re-charge). Either way it persists a `fleet.runner_leases` row carrying the
//! durable event envelope + the claim's monotonic `fencing_token`. It calls the
//! existing leaf helpers rather than refactoring `writepath.run`, so the direct
//! path stays byte-identical at the cost of deliberate orchestration
//! duplication — the shared control-plane abstraction is a follow-up.
//!
//! Faithful, non-atomic: the debit fires here (pre-execution estimate, never
//! re-charged at report). `inline` secrets only.
//!
//! Allocator: handlers run inside the per-request arena (`hx.alloc`). Every
//! resolution output (the claimed session, tenant id, resolved provider, parsed
//! secret bodies, lease id, the acquired envelope) is arena-scoped and
//! reclaimed when the request ends — `assign` already freed the decoded stream
//! event (owned by the Redis client's allocator) before returning.

const std = @import("std");
const logging = @import("log");

const hx_mod = @import("../http/handlers/hx.zig");
const common = @import("../http/handlers/common.zig");
const ec = @import("../errors/error_registry.zig");
const wire = @import("contract");
const protocol = wire.protocol;
const constants = @import("common");
const id_format = @import("../types/id_format.zig");
const assign = @import("assign.zig");
const affinity = @import("affinity.zig");
const lease_row = @import("service_lease_row.zig");
const ZombieSession = @import("zombie_session.zig");
const secrets_resolve = @import("secrets_resolve.zig");
const context_resolve = @import("context_resolve.zig");
const approval_gate = @import("approval_gate.zig");
const rows = @import("event_rows.zig");
const metering = @import("../zombie/metering.zig");
const activity_publisher = @import("../zombie/activity_publisher.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const tenant_provider = @import("../state/tenant_provider.zig");
const metrics_runner = @import("../observability/metrics_runner.zig");
const event_envelope = wire.event_envelope;
const execution_policy = wire.execution_policy;

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_lease);

/// The lease-row billing fields, defined alongside the row write in
/// `service_lease_row.zig` (RULE FLL split); aliased here so the billing
/// helpers keep naming the type.
const Billed = lease_row.Billed;

/// POST /v1/runners/me/leases — claim the next event across all active zombies
/// (sticky-preferred), bill it (or reuse a reclaim's billing), and hand back the
/// work + resolved policy. Always 200: a `LeasePayload` when there is work, else
/// `lease=null` + a backoff hint.
pub fn leaseNext(hx: Hx) void {
    const runner_id = hx.principal.runner_id orelse {
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner identity required");
        return;
    };
    const acq = assign.select(hx, runner_id) orelse return replyNoWork(hx);

    var session = ZombieSession.claimZombie(hx.alloc, acq.zombie_id, hx.ctx.pool) catch |err| {
        log.info("lease_claim_unavailable", .{ .zombie_id = acq.zombie_id, .err = @errorName(err) });
        releaseClaim(hx, acq.zombie_id, acq.fencing_token);
        return replyNoWork(hx);
    };
    defer session.deinit(hx.alloc);

    const billed = resolveBilling(hx, &session, acq) orelse {
        releaseClaim(hx, acq.zombie_id, acq.fencing_token);
        return replyNoWork(hx);
    };

    issueLease(hx, runner_id, &session, acq, billed) catch |err| {
        log.err("lease_issue_failed", .{ .zombie_id = acq.zombie_id, .err = @errorName(err) });
        common.internalDbError(hx.res, hx.req_id);
    };
}

/// Fresh → run the pre-execution write-path billing; reclaim → reuse the prior
/// lease's billing (the original lease already debited; never re-charged).
fn resolveBilling(hx: Hx, session: *ZombieSession, acq: assign.Acquired) ?Billed {
    switch (acq.kind) {
        .reclaim => {
            const r = acq.reused.?;
            return Billed{ .tenant_id = r.tenant_id, .posture = r.posture, .model = r.model };
        },
        .fresh => {
            var event = eventView(acq);
            return runBilling(hx, session, &event);
        },
    }
}

/// A borrowed `ZombieEvent` view over the acquired envelope for the leaf write
/// helpers (read-only — never mutated or freed; the slices are arena-owned).
fn eventView(acq: assign.Acquired) redis_zombie.ZombieEvent {
    return .{
        .event_id = @constCast(acq.event_id),
        .actor = @constCast(acq.actor),
        .event_type = @constCast(acq.event_type),
        .workspace_id = @constCast(acq.workspace_id),
        .request_json = @constCast(acq.request_json),
        .created_at_ms = acq.event_created_at,
    };
}

/// Mirror `event_loop_writepath.run` steps 1–7 via the leaf helpers, then
/// resolve the provider so the caller can build the lease. Any blocked/failed
/// gate returns null → the caller releases the claim and answers no-work. (A
/// blocked-gate event stays un-acked; markBlocked/dead-letter handling is a
/// follow-up, as in the direct path.)
fn runBilling(hx: Hx, session: *ZombieSession, event: *const redis_zombie.ZombieEvent) ?Billed {
    const alloc = hx.alloc;
    const pool = hx.ctx.pool;

    rows.insertReceivedRow(alloc, pool, session, event) catch |err| {
        log.err("lease_received_insert_failed", .{ .zombie_id = session.zombie_id, .event_id = event.event_id, .err = @errorName(err) });
        return null;
    };
    // Open the SSE activity bracket the deleted worker published on receive —
    // the dashboard + `zombiectl steer` consume the `event_received` frame to
    // start the live tail. Best-effort (the publisher swallows failures).
    var scratch = activity_publisher.Scratch.init(alloc);
    defer scratch.deinit();
    activity_publisher.publishEventReceived(hx.ctx.queue, &scratch, session.zombie_id, event.event_id, event.actor);

    var tr = resolveTenant(alloc, pool, session.workspace_id) orelse return null;
    // Own the resolved provider for the whole billing pass: on success it is
    // carried into `Billed` so the lease delivers the SAME key it billed (no
    // second resolve, no rotation TOCTOU); on any gate failure the defer zeroes
    // + frees it (arena teardown does not zero, so the secureZero is load-bearing).
    var committed = false;
    defer if (!committed) tr.resolved.deinit(alloc);
    const ctx = metering.PreflightContext{
        .workspace_id = session.workspace_id,
        .zombie_id = session.zombie_id,
        .event_id = event.event_id,
        .posture = tr.resolved.mode,
        .provider = tr.resolved.provider,
        .model = tr.resolved.model,
    };
    const policy = hx.ctx.balance_policy; // resolved once at startup, carried on the context

    if (!metering.balanceCoversEstimate(pool, alloc, tr.tenant_id, tr.resolved.mode, tr.resolved.provider, tr.resolved.model, policy)) {
        log.info("lease_balance_exhausted", .{ .zombie_id = session.zombie_id });
        return null;
    }
    if (metering.debitReceive(pool, alloc, tr.tenant_id, ctx, policy) != .deducted) return null;
    switch (approval_gate.checkApprovalGate(alloc, session, event, pool, hx.ctx.queue)) {
        .passed => {},
        .pending => {
            // Human decision outstanding: answer no-work; the next lease poll
            // re-evaluates the recorded gate ref. No thread waits.
            log.info("lease_gate_pending", .{ .zombie_id = session.zombie_id, .event_id = event.event_id });
            return null;
        },
        .blocked, .auto_killed => {
            log.info("lease_gate_blocked", .{ .zombie_id = session.zombie_id, .event_id = event.event_id });
            return null;
        },
    }
    // No issue-time stage debit: run fee + tokens meter on /renew + settle at report.

    committed = true; // ownership of tr.resolved transfers to the returned Billed
    return Billed{ .tenant_id = tr.tenant_id, .posture = tr.resolved.mode.label(), .model = tr.resolved.model, .provider = tr.resolved };
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

/// Build the lease payload + persist the `fleet.runner_leases` row (with the
/// durable envelope + the claim's fencing token), then 200.
fn issueLease(hx: Hx, runner_id: []const u8, session: *ZombieSession, acq: assign.Acquired, billed: Billed) !void {
    // Provider key for the lease: a FRESH lease carried it from billing (bill key
    // == deliver key, no second resolve); a reclaim has no billing pass, so
    // re-resolve now (the key is never persisted to the lease row). deinit
    // (secureZero + free) runs after `hx.ok` serializes; set up first so the
    // defer also covers the early-return paths below.
    var resolved: ?tenant_provider.ResolvedProvider = billed.provider;
    if (resolved == null) resolved = resolveProviderForLease(hx, billed.tenant_id);
    defer if (resolved) |*r| r.deinit(hx.alloc);

    const ev_type = event_envelope.EventType.fromSlice(acq.event_type) orelse {
        log.warn("lease_unknown_event_type", .{ .zombie_id = acq.zombie_id, .event_type = acq.event_type });
        releaseClaim(hx, acq.zombie_id, acq.fencing_token);
        return replyNoWork(hx);
    };
    const envelope = event_envelope{
        .event_id = acq.event_id,
        .zombie_id = acq.zombie_id,
        .workspace_id = acq.workspace_id,
        .actor = acq.actor,
        .event_type = ev_type,
        .request_json = acq.request_json,
        .created_at = acq.event_created_at,
    };

    const lease_id = try id_format.generateRunnerLeaseId(hx.alloc);
    try lease_row.insertLeaseRow(hx, runner_id, acq, billed, lease_id);
    metrics_runner.incRunnerActiveLeases(runner_id); // in-memory gauge; decremented on the runner's report

    log.info("lease_issued", .{ .zombie_id = acq.zombie_id, .event_id = acq.event_id, .lease_id = lease_id, .fencing_token = acq.fencing_token, .runner_id = runner_id, .kind = @tagName(acq.kind) });
    hx.ok(.ok, protocol.LeaseResponse{
        .lease = .{
            .lease_id = lease_id,
            .fencing_token = acq.fencing_token,
            .lease_expires_at = acq.leased_until,
            .secret_delivery = .@"inline",
            .event = envelope,
            .policy = resolveExecutionPolicy(hx, session, resolved),
            // The installed SKILL.md body (extracted by ZombieSession), so the runner
            // delivers it to NullClaw. `claimZombie` resolves the session before the
            // fresh/reclaim split, so this is set identically on both paths. Borrowed
            // from `session`, which lives until the response serialises (deinit defer).
            .instructions = session.instructions,
        },
    });
}

/// Resolve the tenant's active provider+key for the lease. Called for BOTH
/// fresh and reclaim leases: `runBilling` discards the key it resolves for
/// metering and the lease row never persists it (plaintext secret in a table is
/// forbidden), so the key must be (re-)resolved here. Reclaim reuses its prior
/// billing — this resolve never re-charges. Returns null on resolve failure;
/// the lease then carries no key and the engine surfaces a clean config error.
/// Caller owns the result and must `deinit` (secureZero) after `hx.ok`.
fn resolveProviderForLease(hx: Hx, tenant_id: []const u8) ?tenant_provider.ResolvedProvider {
    const conn = hx.ctx.pool.acquire() catch |err| {
        log.warn("lease_provider_acquire_failed", .{ .err = @errorName(err) });
        return null;
    };
    defer hx.ctx.pool.release(conn);
    return tenant_provider.resolveActiveProvider(hx.alloc, conn, tenant_id) catch |err| {
        log.warn("lease_provider_key_resolve_failed", .{ .err = @errorName(err) });
        return null;
    };
}

/// `secrets_map` (inline, parsed bodies) + context budget + the resolved
/// provider+key — the resolution `executeInSandbox` does per execution, lifted
/// onto the lease wire. Secret bodies and the provider key are arena-scoped and
/// serialized synchronously by `hx.ok`; they are never logged (Invariant: no
/// secret bytes in logs). `resolved` is owned by the caller and outlives `hx.ok`.
fn resolveExecutionPolicy(hx: Hx, session: *ZombieSession, resolved: ?tenant_provider.ResolvedProvider) execution_policy.ExecutionPolicy {
    const alloc = hx.alloc;
    const budget = context_resolve.resolveContextBudget(session.config.context, session.config.model);
    var secrets_map: ?std.json.Value = null;
    if (session.config.credentials.len > 0) {
        if (secrets_resolve.resolveSecretsMap(alloc, hx.ctx.pool, session.workspace_id, session.config.credentials)) |entries| {
            var obj: std.json.ObjectMap = .empty;
            for (entries) |entry| {
                obj.put(alloc, entry.name, entry.parsed.value) catch |err| log.warn("lease_secret_put_failed", .{ .err = @errorName(err) });
            }
            secrets_map = .{ .object = obj };
        } else |err| {
            log.warn("lease_secrets_resolve_failed", .{ .zombie_id = session.zombie_id, .err = @errorName(err) });
        }
    }
    return .{
        .secrets_map = secrets_map,
        .context = budget,
        .provider = if (resolved) |r| r.provider else "",
        .api_key = if (resolved) |r| r.api_key else "",
    };
}

/// Free the affinity claim won by `assign` when this lease cannot be issued
/// (claim/billing failure), so the zombie is not stuck claimed until its TTL.
/// Token-guarded: frees the slot only while this claim's token is still live.
fn releaseClaim(hx: Hx, zombie_id: []const u8, token: u64) void {
    const conn = hx.ctx.pool.acquire() catch return;
    defer hx.ctx.pool.release(conn);
    affinity.release(conn, zombie_id, token) catch |err| {
        log.warn("lease_claim_release_failed", .{ .zombie_id = zombie_id, .err = @errorName(err) });
    };
}

fn replyNoWork(hx: Hx) void {
    hx.ok(.ok, protocol.LeaseResponse{ .lease = null, .retry_after_ms = constants.NO_WORK_RETRY_AFTER_MS });
}
