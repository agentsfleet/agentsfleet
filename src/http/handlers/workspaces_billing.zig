const std = @import("std");
const httpz = @import("httpz");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const workspace_billing = @import("../../state/workspace_billing.zig");
const telemetry_mod = @import("../../observability/telemetry.zig");
const obs_log = @import("../../observability/logging.zig");
const error_codes = @import("../../errors/codes.zig");
const common = @import("common.zig");
const workspace_guards = @import("../workspace_guards.zig");
const hx_mod = @import("hx.zig");

const log = std.log.scoped(.http);
const API_ACTOR = "api";
const ERR_REQUEST_BODY_REQUIRED = "Request body required";
const ERR_MALFORMED_JSON = "Malformed JSON";

fn parseBillingLifecycleEvent(raw: []const u8) ?workspace_billing.BillingLifecycleEvent {
    if (std.ascii.eqlIgnoreCase(raw, "PAYMENT_FAILED")) return .payment_failed;
    if (std.ascii.eqlIgnoreCase(raw, "DOWNGRADE_TO_FREE")) return .downgrade_to_free;
    return null;
}

fn innerUpgradeWorkspaceToScale(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const Req = struct {
        subscription_id: []const u8,
    };

    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, ERR_REQUEST_BODY_REQUIRED);
        return;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;
    const parsed = std.json.parseFromSlice(Req, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, ERR_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const actor = hx.principal.user_id orelse API_ACTOR;
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, actor, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(hx.alloc);

    log.debug("billing.upgrade_request workspace_id={s}", .{workspace_id});

    const upgraded = workspace_billing.upgradeWorkspaceToScale(conn, hx.alloc, workspace_id, .{
        .subscription_id = parsed.value.subscription_id,
        .actor = actor,
    }) catch |err| switch (err) {
        error.InvalidSubscriptionId => {
            hx.ctx.telemetry.capture(telemetry_mod.ApiErrorWithContext, .{ .distinct_id = hx.principal.user_id orelse "", .error_code = error_codes.ERR_BILLING_INVALID_SUBSCRIPTION_ID, .message = "subscription_id is required", .workspace_id = workspace_id, .request_id = hx.req_id });
            hx.fail(error_codes.ERR_BILLING_INVALID_SUBSCRIPTION_ID, "subscription_id is required");
            return;
        },
        else => {
            log.err("billing.upgrade_fail error_code=UZ-INTERNAL-003 workspace_id={s}", .{workspace_id});
            common.internalOperationError(hx.res, "Failed to upgrade workspace to Scale", hx.req_id);
            return;
        },
    };
    defer hx.alloc.free(upgraded.plan_sku);
    defer if (upgraded.subscription_id) |v| hx.alloc.free(v);

    log.info("billing.upgraded workspace_id={s} plan_sku={s}", .{ workspace_id, upgraded.plan_sku });

    hx.ok(.ok, .{
        .workspace_id = workspace_id,
        .plan_tier = upgraded.plan_tier.label(),
        .billing_status = upgraded.billing_status.label(),
        .plan_sku = upgraded.plan_sku,
        .subscription_id = upgraded.subscription_id,
        .request_id = hx.req_id,
    });
}

pub const handleUpgradeWorkspaceToScale = hx_mod.authenticatedWithParam(innerUpgradeWorkspaceToScale);

fn innerSetWorkspaceScoringConfig(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const Req = struct {
        scoring_context_max_tokens: i32,
    };

    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, ERR_REQUEST_BODY_REQUIRED);
        return;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;
    const parsed = std.json.parseFromSlice(Req, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, ERR_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();

    log.debug("billing.set_scoring_config workspace_id={s} tokens={d}", .{ workspace_id, parsed.value.scoring_context_max_tokens });

    if (parsed.value.scoring_context_max_tokens < 512 or parsed.value.scoring_context_max_tokens > 8192) {
        hx.fail(error_codes.ERR_SCORING_CONTEXT_TOKENS_INVALID, "scoring_context_max_tokens must be between 512 and 8192");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const actor = hx.principal.user_id orelse API_ACTOR;
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, actor, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(hx.alloc);

    var q = PgQuery.from(conn.query(
        \\UPDATE workspace_entitlements
        \\SET scoring_context_max_tokens = $2,
        \\    updated_at = $3
        \\WHERE workspace_id = $1
        \\RETURNING scoring_context_max_tokens
    , .{ workspace_id, parsed.value.scoring_context_max_tokens, std.time.milliTimestamp() }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer q.deinit();

    const row = (q.next() catch null) orelse {
        hx.fail(error_codes.ERR_WORKSPACE_NOT_FOUND, "Workspace not found");
        return;
    };
    const configured_tokens = row.get(i32, 0) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    hx.ok(.ok, .{
        .workspace_id = workspace_id,
        .scoring_context_max_tokens = configured_tokens,
        .request_id = hx.req_id,
    });
}

pub const handleSetWorkspaceScoringConfig = hx_mod.authenticatedWithParam(innerSetWorkspaceScoringConfig);

/// Respond with the billed state and emit the posthog lifecycle event.
fn respondBillingState(hx: hx_mod.Hx, workspace_id: []const u8, event_type: []const u8, reason: []const u8, state: workspace_billing.StateView) void {
    log.info("billing.event_applied workspace_id={s} event_type={s} plan_tier={s}", .{ workspace_id, event_type, state.plan_tier.label() });
    hx.ok(.ok, .{
        .workspace_id = workspace_id,
        .event_type = event_type,
        .plan_tier = state.plan_tier.label(),
        .billing_status = state.billing_status.label(),
        .plan_sku = state.plan_sku,
        .grace_expires_at = state.grace_expires_at,
        .request_id = hx.req_id,
    });
    hx.ctx.telemetry.capture(telemetry_mod.BillingLifecycleEvent, .{
        .distinct_id = telemetry_mod.distinctIdOrSystem(hx.principal.user_id orelse ""),
        .workspace_id = workspace_id,
        .event_type = event_type,
        .reason = reason,
        .plan_tier = state.plan_tier.label(),
        .billing_status = state.billing_status.label(),
        .request_id = hx.req_id,
    });
}

fn innerApplyWorkspaceBillingEvent(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const Req = struct {
        event_type: []const u8,
        reason: []const u8,
    };

    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, ERR_REQUEST_BODY_REQUIRED);
        return;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;
    const parsed = std.json.parseFromSlice(Req, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, ERR_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();

    const event = parseBillingLifecycleEvent(parsed.value.event_type) orelse {
        hx.fail(error_codes.ERR_BILLING_INVALID_EVENT, "Unsupported billing event_type");
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const actor = hx.principal.user_id orelse API_ACTOR;
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, actor, .{
        .minimum_role = .admin,
    }) orelse return;
    defer access.deinit(hx.alloc);

    log.debug("billing.apply_event workspace_id={s} event_type={s}", .{ workspace_id, parsed.value.event_type });

    const state = workspace_billing.applyBillingLifecycleEvent(conn, hx.alloc, workspace_id, .{
        .event = event,
        .reason = parsed.value.reason,
        .actor = actor,
    }) catch |err| {
        if (workspace_billing.errorCode(err)) |code| {
            hx.ctx.telemetry.capture(telemetry_mod.ApiErrorWithContext, .{ .distinct_id = hx.principal.user_id orelse "", .error_code = code, .message = workspace_billing.errorMessage(err) orelse "Workspace billing failure", .workspace_id = workspace_id, .request_id = hx.req_id });
            hx.fail(code, workspace_billing.errorMessage(err) orelse "Workspace billing failure");
            return;
        }
        log.err("billing.apply_event_fail error_code=UZ-INTERNAL-003 workspace_id={s} event_type={s}", .{ workspace_id, parsed.value.event_type });
        common.internalOperationError(hx.res, "Failed to apply workspace billing event", hx.req_id);
        return;
    };
    defer hx.alloc.free(state.plan_sku);
    defer if (state.subscription_id) |v| hx.alloc.free(v);

    respondBillingState(hx, workspace_id, parsed.value.event_type, parsed.value.reason, state);
}

pub const handleApplyWorkspaceBillingEvent = hx_mod.authenticatedWithParam(innerApplyWorkspaceBillingEvent);

test "parseBillingLifecycleEvent accepts supported event types" {
    try std.testing.expectEqual(workspace_billing.BillingLifecycleEvent.payment_failed, parseBillingLifecycleEvent("PAYMENT_FAILED").?);
    try std.testing.expectEqual(workspace_billing.BillingLifecycleEvent.downgrade_to_free, parseBillingLifecycleEvent("DOWNGRADE_TO_FREE").?);
    try std.testing.expect(parseBillingLifecycleEvent("ACTIVATE_SCALE") == null);
}

test "parseBillingLifecycleEvent is case-insensitive" {
    try std.testing.expectEqual(workspace_billing.BillingLifecycleEvent.payment_failed, parseBillingLifecycleEvent("payment_failed").?);
    try std.testing.expectEqual(workspace_billing.BillingLifecycleEvent.payment_failed, parseBillingLifecycleEvent("Payment_Failed").?);
    try std.testing.expectEqual(workspace_billing.BillingLifecycleEvent.payment_failed, parseBillingLifecycleEvent("pAyMeNt_fAiLeD").?);
    try std.testing.expectEqual(workspace_billing.BillingLifecycleEvent.downgrade_to_free, parseBillingLifecycleEvent("downgrade_to_free").?);
    try std.testing.expectEqual(workspace_billing.BillingLifecycleEvent.downgrade_to_free, parseBillingLifecycleEvent("Downgrade_To_Free").?);
}

test "parseBillingLifecycleEvent rejects empty, whitespace, and unicode input" {
    try std.testing.expect(parseBillingLifecycleEvent("") == null);
    try std.testing.expect(parseBillingLifecycleEvent(" ") == null);
    try std.testing.expect(parseBillingLifecycleEvent("  PAYMENT_FAILED  ") == null);
    try std.testing.expect(parseBillingLifecycleEvent("\t") == null);
    try std.testing.expect(parseBillingLifecycleEvent("\n") == null);
    try std.testing.expect(parseBillingLifecycleEvent("\u{00e9}") == null);
    try std.testing.expect(parseBillingLifecycleEvent("\u{0000}") == null);
}

test "parseBillingLifecycleEvent rejects partial matches" {
    try std.testing.expect(parseBillingLifecycleEvent("PAYMENT") == null);
    try std.testing.expect(parseBillingLifecycleEvent("DOWNGRADE") == null);
    try std.testing.expect(parseBillingLifecycleEvent("PAYMENT_FAILED_EXTRA") == null);
    try std.testing.expect(parseBillingLifecycleEvent("DOWNGRADE_TO_FREE_NOW") == null);
    try std.testing.expect(parseBillingLifecycleEvent("_PAYMENT_FAILED") == null);
    try std.testing.expect(parseBillingLifecycleEvent("FAILED") == null);
    try std.testing.expect(parseBillingLifecycleEvent("TO_FREE") == null);
}
