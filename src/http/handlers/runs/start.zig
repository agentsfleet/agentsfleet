const std = @import("std");
const pg = @import("pg");
const httpz = @import("httpz");
const common = @import("../common.zig");
const policy = @import("../../../state/policy.zig");
const entitlements = @import("../../../state/entitlements.zig");
const workspace_billing = @import("../../../state/workspace_billing.zig");
const workspace_credit = @import("../../../state/workspace_credit.zig");
const metrics = @import("../../../observability/metrics.zig");
const trace_ctx = @import("../../../observability/trace.zig");
const obs_log = @import("../../../observability/logging.zig");
const posthog_events = @import("../../../observability/posthog_events.zig");
const profile_linkage = @import("../../../audit/profile_linkage.zig");
const defaults = @import("../../../types/defaults.zig");
const id_format = @import("../../../types/id_format.zig");
const error_codes = @import("../../../errors/codes.zig");
const start_dedup = @import("start_dedup.zig");
const start_budget = @import("start_budget.zig");
const log = std.log.scoped(.http);
const API_ACTOR = "api";
const uc4 = @import("../../../db/test_fixtures_uc4.zig");

const queue_unavailable_code = error_codes.ERR_QUEUE_UNAVAILABLE;
const queue_unavailable_message = "Queue unavailable";

/// Re-export so callers that import start.zig directly can use computeDedupKey.
pub const computeDedupKey = start_dedup.computeDedupKey;

fn enforceRuntimeActiveProfile(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    requested_by: []const u8,
) (entitlements.EnforcementError || anyerror)!void {
    var active_profile = try conn.query(
        \\SELECT wap.config_version_id, v.compiled_profile_json
        \\FROM workspace_active_config wap
        \\JOIN agent_config_versions v ON v.config_version_id = wap.config_version_id
        \\WHERE wap.workspace_id = $1
        \\LIMIT 1
    , .{workspace_id});
    defer active_profile.deinit();

    const row = (try active_profile.next()) orelse return;
    // Copy row-backed slices before drain so enforceWithAudit can use the connection.
    const profile_version_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    const cpj_raw = try row.get(?[]const u8, 1);
    const compiled_profile_json: ?[]const u8 = if (cpj_raw) |v| try alloc.dupe(u8, v) else null;
    try active_profile.drain();
    try entitlements.enforceWithAudit(
        conn,
        alloc,
        workspace_id,
        profile_version_id,
        compiled_profile_json,
        .runtime,
        requested_by,
    );
}

/// Upsert inline spec markdown into the specs table and return the spec_id.
/// file_path is `inline:<first 16 chars of content_hash>`.
/// Caller owns returned slice.
fn upsertInlineSpec(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    spec_markdown: []const u8,
    now_ms: i64,
) ![]u8 {
    var content_hash_hasher = std.crypto.hash.sha2.Sha256.init(.{});
    content_hash_hasher.update(spec_markdown);
    const content_digest = content_hash_hasher.finalResult();
    const content_hash_hex = std.fmt.bytesToHex(content_digest, .lower);

    const file_path = try std.fmt.allocPrint(alloc, "inline:{s}", .{content_hash_hex[0..16]});

    // Title = first 80 chars of spec (trim whitespace, take first line if long).
    const first_line_end = std.mem.indexOfScalar(u8, spec_markdown, '\n') orelse spec_markdown.len;
    const raw_title = std.mem.trim(u8, spec_markdown[0..first_line_end], " \t#");
    const title = if (raw_title.len > 80) raw_title[0..80] else raw_title;

    var q = try conn.query(
        \\INSERT INTO specs (spec_id, workspace_id, tenant_id, file_path, title, status, created_at, updated_at)
        \\SELECT gen_random_uuid(), $1, tenant_id, $2, $3, 'active', $4, $4
        \\FROM workspaces WHERE workspace_id = $1
        \\ON CONFLICT (workspace_id, file_path) DO UPDATE SET updated_at = $4
        \\RETURNING spec_id
    , .{ workspace_id, file_path, title, now_ms });
    defer q.deinit();
    // check-pg-drain: ok — RETURNING 1 row; drained implicitly by deinit after we read it.

    const row = (try q.next()) orelse return error.InlineSpecUpsertFailed;
    const spec_id = try alloc.dupe(u8, try row.get([]u8, 0));
    try q.drain();
    return spec_id;
}

pub fn handleStartRun(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();

    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthErrorWithTracking(res, req_id, err, ctx.posthog);
        return;
    };

    const body = req.body() orelse {
        common.errorResponse(res, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };

    // M16_002: Extended request struct — spec_id OR spec_markdown may be provided.
    // base_commit_sha is used for dedup key computation.
    const Req = struct {
        workspace_id: []const u8,
        spec_id: ?[]const u8 = null,
        spec_markdown: ?[]const u8 = null,
        mode: []const u8,
        requested_by: []const u8,
        idempotency_key: []const u8,
        base_commit_sha: ?[]const u8 = null,
    };

    const parsed = std.json.parseFromSlice(Req, alloc, body, .{ .ignore_unknown_fields = true }) catch {
        common.errorResponse(res, error_codes.ERR_INVALID_REQUEST, "Malformed JSON or missing required fields", req_id);
        return;
    };
    defer parsed.deinit();
    const rval = parsed.value;
    if (!common.requireUuidV7Id(res, req_id, rval.workspace_id, "workspace_id")) return;

    // Exactly one of spec_id or spec_markdown must be provided.
    if (rval.spec_id == null and rval.spec_markdown == null) {
        common.errorResponse(res, error_codes.ERR_INVALID_REQUEST, "Either spec_id or spec_markdown is required", req_id);
        return;
    }
    if (rval.spec_id != null and rval.spec_markdown != null) {
        common.errorResponse(res, error_codes.ERR_INVALID_REQUEST, "Provide spec_id or spec_markdown, not both", req_id);
        return;
    }
    if (rval.spec_id) |sid| {
        if (!common.requireUuidV7Id(res, req_id, sid, "spec_id")) return;
    }

    if (!common.beginApiRequest(ctx)) {
        common.errorResponse(res, error_codes.ERR_API_SATURATED, "Server overloaded; retry shortly", req_id);
        return;
    }
    defer common.endApiRequest(ctx);

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, rval.workspace_id)) {
        common.errorResponse(res, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const billing_state = workspace_billing.reconcileWorkspaceBilling(conn, alloc, rval.workspace_id, std.time.milliTimestamp(), principal.user_id orelse API_ACTOR) catch |err| {
        if (workspace_billing.errorCode(err)) |code| {
            common.errorResponse(res, code, workspace_billing.errorMessage(err) orelse "Workspace billing failure", req_id);
            return;
        }
        common.internalOperationError(res, "Failed to reconcile workspace billing state", req_id);
        return;
    };
    defer alloc.free(billing_state.plan_sku);
    defer if (billing_state.subscription_id) |v| alloc.free(v);
    const credit = workspace_credit.enforceExecutionAllowed(conn, alloc, rval.workspace_id, billing_state.plan_tier) catch |err| {
        if (workspace_credit.errorCode(err)) |code| {
            common.errorResponse(res, code, workspace_credit.errorMessage(err) orelse "Workspace credit failure", req_id);
            return;
        }
        common.internalOperationError(res, "Failed to validate workspace credit balance", req_id);
        return;
    };
    defer alloc.free(credit.currency);

    // M17_001 §2.3-2.4: early budget check (unlocked) before expensive guards.
    // Rejects obviously overbudget workspaces without paying lock costs.
    // The definitive locked check runs inside the INSERT transaction below.
    const budget_early = start_budget.enforceWorkspaceMonthlyBudget(conn, rval.workspace_id) catch {
        common.internalOperationError(res, "Failed to check workspace budget", req_id);
        return;
    };
    if (budget_early) {
        common.errorResponse(res, error_codes.ERR_WORKSPACE_MONTHLY_BUDGET_EXCEEDED, "Workspace monthly token budget exceeded", req_id);
        return;
    }

    {
        var ws_check = conn.query(
            "SELECT paused FROM workspaces WHERE workspace_id = $1",
            .{rval.workspace_id},
        ) catch {
            common.internalDbError(res, req_id);
            return;
        };
        defer ws_check.deinit();

        const ws_row = ws_check.next() catch null orelse {
            common.errorResponse(res, error_codes.ERR_WORKSPACE_NOT_FOUND, "Workspace not found", req_id);
            return;
        };

        const paused = ws_row.get(bool, 0) catch false;
        ws_check.drain() catch {};
        if (paused) {
            common.errorResponse(res, error_codes.ERR_WORKSPACE_PAUSED, "Workspace is paused", req_id);
            return;
        }
    }

    // Resolve spec_id: either provided directly or upserted from inline markdown.
    const now_ms = std.time.milliTimestamp();
    const resolved_spec_id: []const u8 = blk: {
        if (rval.spec_id) |sid| {
            // Validate existing spec.
            var spec_check = conn.query(
                "SELECT spec_id FROM specs WHERE spec_id = $1 AND workspace_id = $2",
                .{ sid, rval.workspace_id },
            ) catch {
                common.internalDbError(res, req_id);
                return;
            };
            defer spec_check.deinit();

            const spec_exists = (spec_check.next() catch null) != null;
            spec_check.drain() catch {};
            if (!spec_exists) {
                common.errorResponse(res, error_codes.ERR_SPEC_NOT_FOUND, "Spec not found", req_id);
                return;
            }
            break :blk sid;
        } else {
            // Upsert inline spec.
            const spec_id = upsertInlineSpec(conn, alloc, rval.workspace_id, rval.spec_markdown.?, now_ms) catch {
                common.internalOperationError(res, "Failed to upsert inline spec", req_id);
                return;
            };
            break :blk spec_id;
        }
    };

    enforceRuntimeActiveProfile(conn, alloc, rval.workspace_id, rval.requested_by) catch |err| switch (err) {
        entitlements.EnforcementError.EntitlementMissing => {
            common.errorResponse(res, error_codes.ERR_ENTITLEMENT_UNAVAILABLE, "Workspace entitlement missing; request denied", req_id);
            return;
        },
        entitlements.EnforcementError.EntitlementProfileLimit => {
            common.errorResponse(res, error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT, "Workspace profile limit exceeded", req_id);
            return;
        },
        entitlements.EnforcementError.EntitlementStageLimit => {
            common.errorResponse(res, error_codes.ERR_ENTITLEMENT_STAGE_LIMIT, "Plan stage limit exceeded", req_id);
            return;
        },
        entitlements.EnforcementError.EntitlementSkillNotAllowed => {
            common.errorResponse(res, error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, "Plan does not allow one or more profile skills", req_id);
            return;
        },
        entitlements.EnforcementError.InvalidCompiledProfile => {
            common.errorResponse(res, error_codes.ERR_PROFILE_INVALID, "Active profile is invalid", req_id);
            return;
        },
        else => {
            common.internalOperationError(res, "Failed to enforce runtime entitlement", req_id);
            return;
        },
    };

    // M16_002 §2.1-2.3: Compute dedup key and check for in-flight duplicate.
    const dedup_key: ?[]u8 = if (rval.spec_markdown) |sm|
        computeDedupKey(alloc, sm, rval.workspace_id, rval.base_commit_sha) catch null
    else
        null;

    if (dedup_key) |dk| {
        const dedup_q = conn.query(
            \\SELECT run_id FROM runs
            \\WHERE dedup_key = $1
            \\  AND state IN ('SPEC_QUEUED', 'RUNNING', 'PLANNED')
            \\LIMIT 1
        , .{dk}) catch null;
        if (dedup_q) |dq| {
            defer dq.deinit();
            // check-pg-drain: ok — at most 1 row; drained in the else branch or by deinit.
            if (dq.next() catch null) |dup_row| {
                const existing_id = alloc.dupe(u8, dup_row.get([]u8, 0) catch "") catch "";
                dq.drain() catch {};
                log.info("run.dedup_hit dedup_key={s} existing_run_id={s}", .{ dk, existing_id });
                res.header("X-Dedup-Run", "existing");
                common.writeJson(res, .ok, .{
                    .run_id = existing_id,
                    .dedup_hit = true,
                    .request_id = req_id,
                });
                return;
            }
            dq.drain() catch {};
        }
    }

    const run_id = id_format.generateRunId(alloc) catch |err| {
        obs_log.logWarnErr(.http, err, "run.id_generation_fail error_code={s}", .{error_codes.ERR_UUIDV7_ID_GENERATION_FAILED});
        common.errorResponse(res, error_codes.ERR_UUIDV7_ID_GENERATION_FAILED, "Failed to generate run identifier", req_id);
        return;
    };

    // Parse W3C traceparent header or generate new trace context.
    const tc = if (req.header("traceparent")) |tp|
        trace_ctx.TraceContext.fromW3CHeader(tp) orelse trace_ctx.TraceContext.generate()
    else
        trace_ctx.TraceContext.generate();
    const trace_id: []const u8 = tc.traceIdSlice();

    const branch = std.fmt.allocPrint(alloc, "zombie/run-{s}", .{run_id}) catch "zombie/run-unknown";

    // M17_001 §2.4: Definitive budget check + INSERT in one transaction.
    // FOR UPDATE lock on workspaces is held until COMMIT, closing the TOCTOU window
    // where two concurrent requests both pass the pre-check and both INSERT.
    _ = conn.exec("BEGIN", .{}) catch {
        common.internalDbError(res, req_id);
        return;
    };
    const budget_final = start_budget.enforceWorkspaceMonthlyBudget(conn, rval.workspace_id) catch {
        _ = conn.exec("ROLLBACK", .{}) catch {};
        common.internalOperationError(res, "Failed to check workspace budget", req_id);
        return;
    };
    if (budget_final) {
        _ = conn.exec("ROLLBACK", .{}) catch {};
        common.errorResponse(res, error_codes.ERR_WORKSPACE_MONTHLY_BUDGET_EXCEEDED, "Workspace monthly token budget exceeded", req_id);
        return;
    }

    var insert = conn.query(
        \\INSERT INTO runs
        \\  (run_id, workspace_id, spec_id, tenant_id, state, attempt, mode,
        \\   requested_by, idempotency_key, request_id, trace_id, branch, run_snapshot_version,
        \\   dedup_key, max_repair_loops, max_tokens, max_wall_time_seconds, created_at, updated_at)
        \\SELECT $1, $2, $3, tenant_id, 'SPEC_QUEUED', 1, $4, $5, $6, $7,
        \\       $8, $9, (SELECT wap.config_version_id FROM workspace_active_config wap WHERE wap.workspace_id = $2),
        \\       $10, $11, $12, $13, $14, $14
        \\FROM workspaces WHERE workspace_id = $2
        \\ON CONFLICT (workspace_id, idempotency_key) DO UPDATE
        \\SET updated_at = runs.updated_at
        \\RETURNING run_id, state, attempt, run_snapshot_version, tenant_id, (xmax = 0) AS inserted
    , .{
        run_id,
        rval.workspace_id,
        resolved_spec_id,
        rval.mode,
        rval.requested_by,
        rval.idempotency_key,
        req_id,
        trace_id,
        branch,
        dedup_key,
        defaults.DEFAULT_RUN_MAX_REPAIR_LOOPS,
        defaults.DEFAULT_RUN_MAX_TOKENS,
        defaults.DEFAULT_RUN_MAX_WALL_TIME_SECONDS,
        now_ms,
    }) catch {
        _ = conn.exec("ROLLBACK", .{}) catch {};
        common.internalOperationError(res, "Failed to create run", req_id);
        return;
    };
    defer insert.deinit();

    const inserted_row = insert.next() catch null orelse {
        _ = conn.exec("ROLLBACK", .{}) catch {};
        common.internalOperationError(res, "Failed to upsert run", req_id);
        return;
    };
    // Copy row-backed slices before drain so subsequent conn queries don't hit ConnectionBusy.
    const final_run_id = alloc.dupe(u8, inserted_row.get([]u8, 0) catch run_id) catch run_id;
    const final_state = alloc.dupe(u8, inserted_row.get([]u8, 1) catch "SPEC_QUEUED") catch "SPEC_QUEUED";
    const final_attempt = inserted_row.get(i32, 2) catch 1;
    const run_snapshot_version_raw = inserted_row.get(?[]u8, 3) catch null;
    const run_snapshot_version: ?[]u8 = if (run_snapshot_version_raw) |v| alloc.dupe(u8, v) catch null else null;
    const tenant_id = alloc.dupe(u8, inserted_row.get([]u8, 4) catch "") catch "";
    const was_inserted = inserted_row.get(bool, 5) catch false;
    insert.drain() catch {};

    _ = conn.exec("COMMIT", .{}) catch {
        common.internalDbError(res, req_id);
        return;
    };

    if (!id_format.isSupportedRunId(final_run_id)) {
        common.errorResponse(res, error_codes.ERR_UUIDV7_CANONICAL_FORMAT, "Non-canonical run_id persisted", req_id);
        return;
    }

    if (was_inserted) {
        policy.recordPolicyEvent(conn, rval.workspace_id, final_run_id, .sensitive, .allow, "m1.start_run", rval.requested_by) catch |err| {
            obs_log.logWarnErr(.http, err, "run.policy_event_insert_fail run_id={s}", .{final_run_id});
        };

        log.info("run.created run_id={s} workspace_id={s} spec_id={s}", .{
            final_run_id, rval.workspace_id, resolved_spec_id,
        });
        if (run_snapshot_version) |snapshot| {
            profile_linkage.insertRunArtifact(conn, tenant_id, rval.workspace_id, final_run_id, snapshot, now_ms) catch |err| {
                obs_log.logWarnErr(.http, err, "run.linkage_artifact_fail run_id={s}", .{final_run_id});
                common.compensateStartRunQueueFailure(conn, final_run_id);
                common.internalOperationError(res, "Failed to persist run linkage artifact", req_id);
                return;
            };
        }
        ctx.queue.xaddRun(final_run_id, 0, rval.workspace_id) catch |err| {
            obs_log.logWarnErr(.http, err, "run.queue_enqueue_fail run_id={s} workspace_id={s}", .{
                final_run_id,
                rval.workspace_id,
            });
            common.compensateStartRunQueueFailure(conn, final_run_id);
            common.errorResponse(res, queue_unavailable_code, queue_unavailable_message, req_id);
            return;
        };
        posthog_events.trackRunStarted(
            ctx.posthog,
            posthog_events.distinctIdOrSystem(principal.user_id orelse ""),
            final_run_id,
            rval.workspace_id,
            resolved_spec_id,
            rval.mode,
            req_id,
        );
        metrics.incRunsCreated();
    } else {
        log.info("run.idempotent_replay run_id={s} workspace_id={s}", .{ final_run_id, rval.workspace_id });
    }

    common.writeJson(res, .accepted, .{
        .run_id = final_run_id,
        .state = final_state,
        .attempt = @as(u32, @intCast(final_attempt)),
        .run_snapshot_version = run_snapshot_version,
        .plan_tier = billing_state.plan_tier.label(),
        .billing_status = billing_state.billing_status.label(),
        .credit_remaining_cents = credit.remaining_credit_cents,
        .credit_currency = credit.currency,
        .request_id = req_id,
        .trace_id = trace_id,
    });
}

// T7/T8 computeDedupKey unit tests live in start_dedup.zig.

test "runtime entitlement enforcement rejects downgraded free workspace using scale-only active profile" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // donald-duck has a 4-stage scale profile but scrooges free-plan cap is 3 stages.
    try uc4.seed(db_ctx.conn);
    defer uc4.teardown(db_ctx.conn);

    // Arena ensures internal allocations are freed even when the function returns an error.
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();

    try std.testing.expectError(
        entitlements.EnforcementError.EntitlementStageLimit,
        enforceRuntimeActiveProfile(
            db_ctx.conn,
            arena.allocator(),
            uc4.WS_ID,
            "test",
        ),
    );
}
