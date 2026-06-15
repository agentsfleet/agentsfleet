//! agentsfleetd-side runner control-plane orchestration — the `lease` verb.
//!
//! `leaseNext` delegates assignment to `assign.select`: across all active
//! agents it atomically CLAIMS one (sticky-preferred), then either reclaims an
//! expired holder's event or pulls a fresh one. The pre-execution billing +
//! gate pass (insert-received → resolve tenant/provider → balance gate → debit
//! receive → approval gate, plus the terminal `gate_blocked` writes for
//! non-retryable refusals) lives in `service_billing.zig` (RULE FLL split);
//! this file keeps the lease build: `secrets_map`/context-budget resolution
//! lifted from `executeInSandbox`, the `fleet.runner_leases` row carrying the
//! durable event envelope + the claim's monotonic `fencing_token`, and the
//! 200 response. A agent that declares credentials never receives a lease
//! without them — a missing secret refuses the lease with a terminal row
//! instead of shipping a silent null map (RULE ESO).
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
const billing = @import("service_billing.zig");
const lease_row = @import("service_lease_row.zig");
const AgentSession = @import("agent_session.zig");
const secrets_resolve = @import("secrets_resolve.zig");
const context_resolve = @import("context_resolve.zig");
const rows = @import("event_rows.zig");
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

/// POST /v1/runners/me/leases — claim the next event across all active agents
/// (sticky-preferred), bill it (or reuse a reclaim's billing), and hand back the
/// work + resolved policy. Always 200: a `LeasePayload` when there is work, else
/// `lease=null` + a backoff hint.
pub fn leaseNext(hx: Hx) void {
    const runner_id = hx.principal.runner_id orelse {
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner identity required");
        return;
    };
    const acq = assign.select(hx, runner_id) orelse return replyNoWork(hx);

    var session = AgentSession.claimAgent(hx.alloc, acq.agent_id, hx.ctx.pool) catch |err| {
        log.info("lease_claim_unavailable", .{ .agent_id = acq.agent_id, .err = @errorName(err) });
        releaseClaim(hx, acq.agent_id, acq.fencing_token);
        return replyNoWork(hx);
    };
    defer session.deinit(hx.alloc);

    const billed = billing.resolveBilling(hx, &session, acq) orelse {
        releaseClaim(hx, acq.agent_id, acq.fencing_token);
        return replyNoWork(hx);
    };

    issueLease(hx, runner_id, &session, acq, billed) catch |err| {
        log.err("lease_issue_failed", .{ .agent_id = acq.agent_id, .err = @errorName(err) });
        common.internalDbError(hx.res, hx.req_id);
    };
}

/// Build the lease payload + persist the `fleet.runner_leases` row (with the
/// durable envelope + the claim's fencing token), then 200.
fn issueLease(hx: Hx, runner_id: []const u8, session: *AgentSession, acq: assign.Acquired, billed: Billed) !void {
    // Provider key for the lease: a FRESH lease carried it from billing (bill key
    // == deliver key, no second resolve); a reclaim has no billing pass, so
    // re-resolve now (the key is never persisted to the lease row). deinit
    // (secureZero + free) runs after `hx.ok` serializes; set up first so the
    // defer also covers the early-return paths below.
    var resolved: ?tenant_provider.ResolvedProvider = billed.provider;
    if (resolved == null) resolved = resolveProviderForLease(hx, billed.tenant_id);
    defer if (resolved) |*r| r.deinit(hx.alloc);

    const ev_type = event_envelope.EventType.fromSlice(acq.event_type) orelse {
        log.warn("lease_unknown_event_type", .{ .agent_id = acq.agent_id, .event_type = acq.event_type });
        releaseClaim(hx, acq.agent_id, acq.fencing_token);
        return replyNoWork(hx);
    };
    // Resolve declared secrets BEFORE building the lease: a missing credential
    // refuses the lease with a terminal row (RULE ESO — no lease ships with a
    // silent null secrets map); a transient vault/DB failure refuses without a
    // terminal write so the delivery stays leasable (RULE ECL). Entries are
    // arena-scoped and serialized synchronously by `hx.ok`.
    const secret_entries: ?[]secrets_resolve.ResolvedSecret = blk: {
        if (session.config.credentials.len == 0) break :blk null;
        break :blk secrets_resolve.resolveSecretsMap(hx.alloc, hx.ctx.pool, session.workspace_id, session.config.credentials) catch |err| {
            if (err == error.CredentialNotFound) {
                log.warn("lease_secret_missing", .{ .agent_id = acq.agent_id, .event_id = acq.event_id });
                billing.blockEvent(hx, acq.agent_id, acq.event_id, rows.LABEL_SECRET_MISSING);
            } else {
                log.warn("lease_secrets_resolve_failed", .{ .agent_id = acq.agent_id, .event_id = acq.event_id, .err = @errorName(err) });
            }
            releaseClaim(hx, acq.agent_id, acq.fencing_token);
            return replyNoWork(hx);
        };
    };
    const envelope = event_envelope{
        .event_id = acq.event_id,
        .agent_id = acq.agent_id,
        .workspace_id = acq.workspace_id,
        .actor = acq.actor,
        .event_type = ev_type,
        .request_json = acq.request_json,
        .created_at = acq.event_created_at,
    };

    const lease_id = try id_format.generateRunnerLeaseId(hx.alloc);
    try lease_row.insertLeaseRow(hx, runner_id, acq, billed, lease_id);
    metrics_runner.incRunnerActiveLeases(runner_id); // in-memory gauge; decremented on the runner's report

    log.info("lease_issued", .{ .agent_id = acq.agent_id, .event_id = acq.event_id, .lease_id = lease_id, .fencing_token = acq.fencing_token, .runner_id = runner_id, .kind = @tagName(acq.kind) });
    hx.ok(.ok, protocol.LeaseResponse{
        .lease = .{
            .lease_id = lease_id,
            .fencing_token = acq.fencing_token,
            .lease_expires_at = acq.leased_until,
            .secret_delivery = .@"inline",
            .event = envelope,
            .policy = resolveExecutionPolicy(hx, session, resolved, secret_entries),
            // The installed SKILL.md body (extracted by AgentSession), so the runner
            // delivers it to NullClaw. `claimAgent` resolves the session before the
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

/// `secrets_map` (inline, pre-resolved parsed bodies from `issueLease`) +
/// context budget + the resolved provider+key — the resolution
/// `executeInSandbox` does per execution, lifted onto the lease wire. Secret
/// bodies and the provider key are arena-scoped and serialized synchronously
/// by `hx.ok`; they are never logged (Invariant: no secret bytes in logs).
/// `resolved` is owned by the caller and outlives `hx.ok`. Resolution failures
/// refused the lease upstream — by here `entries` is complete or absent.
fn resolveExecutionPolicy(hx: Hx, session: *AgentSession, resolved: ?tenant_provider.ResolvedProvider, entries: ?[]secrets_resolve.ResolvedSecret) execution_policy.ExecutionPolicy {
    const alloc = hx.alloc;
    const budget = context_resolve.resolveContextBudget(session.config.context, session.config.model);
    var secrets_map: ?std.json.Value = null;
    if (entries) |list| {
        var obj: std.json.ObjectMap = .empty;
        for (list) |entry| {
            obj.put(alloc, entry.name, entry.parsed.value) catch |err| log.warn("lease_secret_put_failed", .{ .err = @errorName(err) });
        }
        secrets_map = .{ .object = obj };
    }
    return .{
        .secrets_map = secrets_map,
        .context = budget,
        .provider = if (resolved) |r| r.provider else "",
        .api_key = if (resolved) |r| r.api_key else "",
    };
}

/// Free the affinity claim won by `assign` when this lease cannot be issued
/// (claim/billing failure), so the agent is not stuck claimed until its TTL.
/// Token-guarded: frees the slot only while this claim's token is still live.
fn releaseClaim(hx: Hx, agent_id: []const u8, token: u64) void {
    const conn = hx.ctx.pool.acquire() catch return;
    defer hx.ctx.pool.release(conn);
    affinity.release(conn, agent_id, token) catch |err| {
        log.warn("lease_claim_release_failed", .{ .agent_id = agent_id, .err = @errorName(err) });
    };
}

fn replyNoWork(hx: Hx) void {
    hx.ok(.ok, protocol.LeaseResponse{ .lease = null, .retry_after_ms = constants.NO_WORK_RETRY_AFTER_MS });
}
