//! Every wallet and ledger statement one telemetry event issues, under a
//! single `billing_runtime` elevation.
//!
//! The debit, the exhaustion mark and the ledger row are all billing
//! statements against the same connection inside the same transaction, so
//! bracketing them together costs one `SET LOCAL ROLE` pair per event rather
//! than one per store call. Each store function keeps its standalone form for
//! callers that issue only that statement; the `*Elevated` forms this span
//! calls take the handle instead of opening their own scope.
//!
//! The span never commits or rolls back: the caller owns the transaction (it
//! spans more than billing) and decides on the returned outcome.

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");
const pool_elevation = @import("../db/pool_elevation.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const tenant_provider = @import("../state/tenant_provider.zig");
const fleet_telemetry_store = @import("../state/fleet_telemetry_store.zig");

const Allocator = std.mem.Allocator;
const log = logging.scoped(.fleet_metering);

/// What the charge is being raised against — resolved before the debit so the
/// span binds identifiers rather than re-reading them.
pub const PreflightContext = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
    posture: tenant_provider.Mode,
    provider: []const u8,
    model: []const u8,
};

/// What the span decided. The caller owns the transaction, so it — not the
/// span — commits or rolls back on this.
pub const SpanOutcome = enum { debited, exhausted, missing_tenant_billing };

pub const BillingSpan = struct {
    alloc: Allocator,
    tenant_id: []const u8,
    ctx: PreflightContext,
    event_created_at: i64,
    charge_type: fleet_telemetry_store.ChargeType,
    nanos: i64,

    /// Debit, then write the ledger row. Each failure is logged where it is
    /// detected — the caller sees only the outcome, so the specific statement
    /// that failed has to be named here or it is lost.
    pub fn run(c: BillingSpan, v: pool_elevation.Elevated(.billing)) !SpanOutcome {
        if (c.nanos > 0) {
            _ = tenant_billing.debitElevated(v, c.tenant_id, c.nanos) catch |err| switch (err) {
                error.CreditExhausted => {
                    _ = tenant_billing.markExhaustedElevated(v, c.tenant_id) catch |mark_err| {
                        log.warn("mark_exhausted_fail", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = c.ctx.fleet_id, .tenant_id = c.tenant_id, .err = @errorName(mark_err) });
                    };
                    // No ledger row follows: nothing was charged.
                    return .exhausted;
                },
                error.TenantBillingMissing => {
                    log.err("missing_tenant_billing", .{
                        .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
                        .fleet_id = c.ctx.fleet_id,
                        .tenant_id = c.tenant_id,
                        .workspace_id = c.ctx.workspace_id,
                        .msg = "starter grant was never inserted for this tenant",
                    });
                    return .missing_tenant_billing;
                },
                else => {
                    log.warn("debit_fail", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = c.ctx.fleet_id, .tenant_id = c.tenant_id, .err = @errorName(err) });
                    return err;
                },
            };
        }
        return c.writeLedgerRow(v);
    }

    fn writeLedgerRow(c: BillingSpan, v: pool_elevation.Elevated(.billing)) !SpanOutcome {
        fleet_telemetry_store.insertTelemetryElevated(v, c.alloc, .{
            .tenant_id = c.tenant_id,
            .workspace_id = c.ctx.workspace_id,
            .fleet_id = c.ctx.fleet_id,
            .event_id = c.ctx.event_id,
            .charge_type = c.charge_type,
            .posture = c.ctx.posture,
            .model = c.ctx.model,
            .credit_deducted_nanos = c.nanos,
            .token_count_input = null,
            .token_count_cached_input = null,
            .token_count_output = null,
            .wall_ms = null,
            .event_created_at = c.event_created_at,
            .created_at = clock.nowMillis(),
        }) catch |err| {
            log.warn("telemetry_insert_fail", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = c.ctx.fleet_id, .event_id = c.ctx.event_id, .charge_type = c.charge_type.label(), .err = @errorName(err) });
            return err;
        };
        return .debited;
    }
};
