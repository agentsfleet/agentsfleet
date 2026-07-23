//! Pre-execution billing + gate pass for the lease verb (RULE FLL split from
//! service.zig). Mirrors `event_loop_writepath.run` steps 1–7 via the leaf
//! helpers: insert-received → resolve tenant/provider → balance gate → debit
//! receive → approval gate. A refusal that is not retryable-by-waiting writes
//! the documented terminal half (scenario 03): a `gate_blocked` row with a
//! named failure label, the `event_complete` frame, then XACK — the row commit
//! lands strictly before the XACK, so a crash between the two leaves the
//! delivery reclaimable, never acked-and-lost. Transient failures (pool/Redis
//! blips, gate backend unavailable) return no-work WITHOUT a terminal write —
//! the delivery stays leasable and the next poll retries (RULE ECL).
//!
//! Allocator: per-request arena (`hx.alloc`); see service.zig's module note.

const std = @import("std");
const logging = @import("log");
const clock = @import("common").clock;
const ec = @import("../errors/error_registry.zig");
const pg = @import("pg");
const budget = @import("budget.zig");

const hx_mod = @import("../http/handlers/hx.zig");
const assign = @import("assign.zig");
const lease_row = @import("service_lease_row.zig");
const FleetSession = @import("fleet_session.zig");
const approval_gate = @import("approval_gate.zig");
const rows = @import("event_rows.zig");
const metering = @import("../fleet_runtime/metering.zig");
const otel_metrics = @import("../observability/otel_metrics.zig");
const activity_publisher = @import("../fleet_runtime/activity_publisher.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const tenant_provider = @import("../state/tenant_provider.zig");

const Hx = hx_mod.Hx;
const Billed = lease_row.Billed;
// service.zig sibling — one logical module split for the length gate.
const log = logging.scoped(.runner_lease);

/// Fresh → run the pre-execution write-path billing; reclaim → reuse the prior
/// lease's billing (the original lease already debited; never re-charged).
pub fn resolveBilling(hx: Hx, session: *FleetSession, acq: assign.Acquired) ?Billed {
    switch (acq.kind) {
        .reclaim => {
            const r = acq.reused.?;
            return .{ .tenant_id = r.tenant_id, .posture = r.posture, .model = r.model };
        },
        .fresh => {
            var event = eventView(acq);
            return runBilling(hx, session, &event);
        },
    }
}

/// The documented terminal half for a non-retryable gate refusal: write the
/// `gate_blocked` row (guarded; commit BEFORE XACK), publish the completion
/// frame so live tails close, then XACK. A failed row write skips the XACK so
/// the delivery stays reclaimable. Zero rows affected means the row was
/// already terminal (a refused re-delivery whose earlier XACK was lost) — the
/// XACK is still owed.
pub fn blockEvent(hx: Hx, fleet_id: []const u8, event_id: []const u8, label: []const u8) void {
    const affected = rows.markBlocked(hx.ctx.pool, fleet_id, event_id, label) catch |err| {
        log.warn("lease_block_write_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = fleet_id, .event_id = event_id, .failure_label = label, .err = @errorName(err) });
        return;
    };
    var scratch = activity_publisher.Scratch.init(hx.alloc);
    defer scratch.deinit();
    activity_publisher.publishEventComplete(hx.ctx.queue, &scratch, fleet_id, event_id, rows.STATUS_GATE_BLOCKED, .{ .label = label });
    redis_fleet.xackFleet(hx.ctx.queue, fleet_id, event_id) catch |err| {
        log.warn("lease_block_xack_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id, .event_id = event_id, .err = @errorName(err) });
    };
    log.debug("lease_gate_blocked", .{ .fleet_id = fleet_id, .event_id = event_id, .failure_label = label, .rows_affected = affected });
}

/// A borrowed `FleetEvent` view over the acquired envelope for the leaf write
/// helpers (read-only — never mutated or freed; the slices are arena-owned).
fn eventView(acq: assign.Acquired) redis_fleet.FleetEvent {
    return .{
        .event_id = @constCast(acq.event_id),
        .actor = @constCast(acq.actor),
        .event_type = @constCast(acq.event_type),
        .workspace_id = @constCast(acq.workspace_id),
        .request_json = @constCast(acq.request_json),
        .created_at_ms = acq.event_created_at,
    };
}

/// The fleet's own spend ceiling, resolved from the config the session already
/// carries — no lookup. A spend that could not be read admits the event
/// (`budget.verdictOrAdmit`), matching `balanceCoversEstimate`'s fail-open
/// posture: a metering outage must not halt every fleet on the platform.
fn budgetVerdict(pool: *pg.Pool, session: *FleetSession, event: *const redis_fleet.FleetEvent) budget.Verdict {
    const spend = budget.spendForFleet(pool, session.workspace_id, session.fleet_id, clock.nowMillis());
    if (spend == null) {
        log.warn("lease_budget_unavailable", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = session.fleet_id, .event_id = event.event_id });
    }
    return budget.verdictOrAdmit(spend, session.config.budget);
}

/// Mirror `event_loop_writepath.run` steps 1–7 via the leaf helpers, then
/// resolve the provider so the caller can build the lease. Permanent refusals
/// (balance exhausted, unresolvable tenant/provider, gate denied/expired)
/// write the terminal `gate_blocked` row + XACK via `blockEvent` and return
/// null; transient failures return null with no terminal write so the
/// delivery stays leasable. The caller releases the claim and answers no-work
/// on every null.
fn runBilling(hx: Hx, session: *FleetSession, event: *const redis_fleet.FleetEvent) ?Billed {
    const alloc = hx.alloc;
    const pool = hx.ctx.pool;

    const first_delivery = rows.insertReceivedRow(alloc, pool, session, event) catch |err| {
        log.err("lease_received_insert_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = session.fleet_id, .event_id = event.event_id, .err = @errorName(err) });
        return null;
    };
    if (!first_delivery) {
        // PEL re-delivery. A row that is already terminal (a settled or
        // gate_blocked entry whose report/block XACK was lost) must be
        // re-acked, never re-leased — re-running a settled lease double-fires
        // side effects and re-meters tokens (spec Invariant 2). A still
        // `received` row is a legitimate re-poll (pending-gate, reclaimed
        // strand): fall through to re-evaluate the gates.
        const klass = rows.classifyStatus(pool, session.fleet_id, event.event_id) catch |err| {
            // Uncertain: leave the entry pending (no XACK, no lease) so the
            // next poll retries the classification rather than risking a
            // double-execution on a guess.
            log.warn("lease_status_classify_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = session.fleet_id, .event_id = event.event_id, .err = @errorName(err) });
            return null;
        };
        if (klass == .terminal) {
            redis_fleet.xackFleet(hx.ctx.queue, session.fleet_id, event.event_id) catch |err| {
                log.warn("lease_terminal_reack_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = session.fleet_id, .event_id = event.event_id, .err = @errorName(err) });
            };
            log.debug("lease_terminal_redelivery_acked", .{ .fleet_id = session.fleet_id, .event_id = event.event_id });
            return null;
        }
    }
    if (first_delivery) {
        // Open the SSE activity bracket exactly once per event — a PEL
        // re-delivery (pending-gate re-poll, reclaimed strand) must not emit
        // a duplicate `event_received` frame. Best-effort.
        var scratch = activity_publisher.Scratch.init(alloc);
        defer scratch.deinit();
        activity_publisher.publishEventReceived(hx.ctx.queue, &scratch, session.fleet_id, event.event_id, event.actor);
    }

    var tr = switch (resolveTenant(alloc, pool, session.workspace_id)) {
        .ok => |t| t,
        .failed_permanent => {
            blockEvent(hx, session.fleet_id, event.event_id, rows.LABEL_TENANT_RESOLVE_FAILED);
            return null;
        },
        .failed_transient => return null,
    };
    // Own the resolved provider for the whole billing pass: on success it is
    // carried into `Billed.provider` and consumed by `issueLease` (service.zig:93
    // assigns `resolved = billed.provider`; the `if (resolved == null)` re-resolve
    // there is reclaim-only — reused sessions never billed), so a FRESH lease
    // delivers the SAME key it billed: no second resolve, no rotation TOCTOU.
    // issueLease's own `defer r.deinit` secureZeros it. On any gate failure here
    // the defer below zeroes + frees it instead (arena teardown does not zero, so
    // the secureZero is load-bearing).
    var committed = false;
    defer if (!committed) tr.resolved.deinit(alloc);
    const ctx = metering.PreflightContext{
        .workspace_id = session.workspace_id,
        .fleet_id = session.fleet_id,
        .event_id = event.event_id,
        .posture = tr.resolved.mode,
        .provider = tr.resolved.provider,
        .model = tr.resolved.model,
    };
    const policy = hx.ctx.balance_policy; // resolved once at startup, carried on the context

    if (!metering.balanceCoversEstimate(pool, alloc, tr.tenant_id, tr.resolved.mode, tr.resolved.provider, tr.resolved.model, policy)) {
        log.debug("lease_balance_exhausted", .{ .fleet_id = session.fleet_id, .event_id = event.event_id });
        blockEvent(hx, session.fleet_id, event.event_id, rows.LABEL_BALANCE_EXHAUSTED);
        return null;
    }
    // The fleet's OWN ceiling, checked after the tenant's credit pool and BEFORE
    // the receive debit below — a refused event must never be charged. Independent
    // of the credit gate: both must pass. Fail-open on a DB fault (`spendForFleet`
    // returns null), mirroring `balanceCoversEstimate`.
    const budget_verdict = budgetVerdict(pool, session, event);
    if (budget_verdict.refused()) {
        log.debug("lease_budget_breach", .{ .error_code = ec.ERR_RUN_BUDGET_EXCEEDED, .fleet_id = session.fleet_id, .event_id = event.event_id, .verdict = @tagName(budget_verdict) });
        blockEvent(hx, session.fleet_id, event.event_id, rows.LABEL_BUDGET_BREACH);
        return null;
    }
    // Receive debits exactly once per event — a re-delivered entry already
    // paid on its first delivery (the balance debit is not replay-guarded;
    // only the telemetry row is).
    if (first_delivery) switch (metering.debitReceive(pool, alloc, tr.tenant_id, ctx, policy)) {
        // Post-commit, fire-and-forget OTLP metric: the receive credit drain.
        // The debit already committed inside debitReceive; this never blocks it.
        .deducted => |nanos| otel_metrics.recordCreditDrain(nanos, ctx.posture.label()),
        .exhausted => {
            blockEvent(hx, session.fleet_id, event.event_id, rows.LABEL_BALANCE_EXHAUSTED);
            return null;
        },
        // Operator-fixable bootstrap gap / transient DB fault: no terminal
        // write, no XACK — the delivery redelivers once the fault clears.
        .missing_tenant_billing, .db_error => {
            log.warn("lease_receive_debit_unavailable", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = session.fleet_id, .event_id = event.event_id });
            return null;
        },
    };
    switch (approval_gate.checkApprovalGate(alloc, session, event, pool, hx.ctx.queue)) {
        .passed => {},
        .pending => {
            // Human decision outstanding: answer no-work; the next lease poll
            // re-delivers the entry from the PEL and re-evaluates the recorded
            // gate ref. No thread waits.
            log.debug("lease_gate_pending", .{ .fleet_id = session.fleet_id, .event_id = event.event_id });
            return null;
        },
        .blocked => |reason| switch (reason) {
            .approval_denied => {
                blockEvent(hx, session.fleet_id, event.event_id, rows.LABEL_APPROVAL_DENIED);
                return null;
            },
            .timeout => {
                blockEvent(hx, session.fleet_id, event.event_id, rows.LABEL_APPROVAL_EXPIRED);
                return null;
            },
            // Redis-unavailable default-deny is transient: no terminal write;
            // the entry stays leasable and the next poll retries the gate.
            .unavailable => {
                log.warn("lease_gate_unavailable", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = session.fleet_id, .event_id = event.event_id });
                return null;
            },
        },
        .auto_killed => |trigger| {
            // The gate paused the fleet; the event is retained un-acked so a
            // resume re-delivers it (Failure Modes: paused mid-flight).
            log.debug("lease_gate_auto_killed", .{ .fleet_id = session.fleet_id, .event_id = event.event_id, .trigger = @tagName(trigger) });
            return null;
        },
    }
    // No issue-time stage debit: run fee + tokens meter on /renew + settle at report.

    committed = true; // ownership of tr.resolved transfers to the returned Billed
    return .{ .tenant_id = tr.tenant_id, .posture = tr.resolved.mode.label(), .model = tr.resolved.model, .provider = tr.resolved };
}

/// Tenant + provider resolution split by error class (RULE ECL): a permanent
/// config failure earns the terminal `gate_blocked` row; a transient infra
/// failure leaves the delivery leasable for the next poll.
const TenantResolution = union(enum) {
    ok: struct { tenant_id: []u8, resolved: tenant_provider.ResolvedProvider },
    failed_permanent: void,
    failed_transient: void,
};

/// One pooled connection resolves tenant id then active provider, mirroring
/// `event_loop_writepath_resolve.resolveTenantAndProvider`'s drain order.
fn resolveTenant(alloc: std.mem.Allocator, pool: *pg.Pool, workspace_id: []const u8) TenantResolution {
    const conn = pool.acquire() catch |err| {
        log.warn("lease_resolve_acquire_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .err = @errorName(err) });
        return .{ .failed_transient = {} };
    };
    defer pool.release(conn);
    const tenant_id = tenant_billing.resolveTenantFromWorkspace(conn, alloc, workspace_id) catch |err| {
        log.err("lease_tenant_lookup_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .workspace_id = workspace_id, .err = @errorName(err) });
        return if (err == error.WorkspaceNotFound) .{ .failed_permanent = {} } else .{ .failed_transient = {} };
    };
    const resolved = tenant_provider.resolveActiveProvider(alloc, conn, tenant_id) catch |err| {
        log.warn("lease_provider_resolve_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .workspace_id = workspace_id, .err = @errorName(err) });
        return switch (err) {
            error.SecretMissing,
            error.SecretDataMalformed,
            error.PlatformKeyMissing,
            error.TenantHasNoWorkspace,
            => .{ .failed_permanent = {} },
            else => .{ .failed_transient = {} },
        };
    };
    return .{ .ok = .{ .tenant_id = tenant_id, .resolved = resolved } };
}
