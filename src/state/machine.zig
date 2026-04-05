//! State machine — validates transitions and writes to run_transitions.
//! All writes are append-only. The current state is derived from the
//! latest transition for a given run_id.

const std = @import("std");
const pg = @import("pg");
const types = @import("../types.zig");
const id_format = @import("../types/id_format.zig");
const events = @import("../events/bus.zig");
const metrics = @import("../observability/metrics.zig");
const obs_log = @import("../observability/logging.zig");
const log = std.log.scoped(.state);

pub const TransitionError = error{
    InvalidTransition,
    RunNotFound,
    RunAlreadyTerminal,
    IdempotencyConflict,
};

const OutboxStatus = enum {
    pending,
    delivered,
    dead_letter,
};

pub fn outboxStatusLabel(status: OutboxStatus) []const u8 {
    return switch (status) {
        .pending => "pending",
        .delivered => "delivered",
        .dead_letter => "dead_letter",
    };
}

pub fn shouldReconcileSideEffectsForState(to: types.RunState) bool {
    return switch (to) {
        .SPEC_QUEUED, .BLOCKED, .NOTIFIED_BLOCKED, .DONE, .CANCELLED, .ABORTED => true,
        else => false,
    };
}

pub fn deadLetterReasonForState(to: types.RunState) []const u8 {
    return switch (to) {
        .SPEC_QUEUED => "reconciled_on_requeue",
        .BLOCKED => "reconciled_on_blocked",
        .NOTIFIED_BLOCKED => "reconciled_on_notified_blocked",
        .DONE => "reconciled_on_done",
        .CANCELLED => "reconciled_on_cancelled",
        .ABORTED => "reconciled_on_aborted",
        else => "reconciled",
    };
}

fn upsertSideEffectOutbox(
    conn: *pg.Conn,
    run_id: []const u8,
    effect_key: []const u8,
    status: OutboxStatus,
    last_event: []const u8,
    payload: ?[]const u8,
    reconciled_state: ?[]const u8,
    now_ms: i64,
) !void {
    const outbox_id = try id_format.generateOutboxId(conn._allocator);
    defer conn._allocator.free(outbox_id);
    _ = try conn.exec(
        \\INSERT INTO run_side_effect_outbox
        \\  (id, run_id, effect_key, status, last_event, payload, reconciled_state, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
        \\ON CONFLICT (run_id, effect_key) DO UPDATE
        \\SET status = EXCLUDED.status,
        \\    last_event = EXCLUDED.last_event,
        \\    payload = COALESCE(EXCLUDED.payload, run_side_effect_outbox.payload),
        \\    reconciled_state = EXCLUDED.reconciled_state,
        \\    updated_at = EXCLUDED.updated_at
    , .{
        outbox_id,
        run_id,
        effect_key,
        outboxStatusLabel(status),
        last_event,
        payload,
        reconciled_state,
        now_ms,
    });
}

pub fn reconcileSideEffectsForRunState(
    conn: *pg.Conn,
    run_id: []const u8,
    to: types.RunState,
) !u32 {
    if (!shouldReconcileSideEffectsForState(to)) return 0;

    const now_ms = std.time.milliTimestamp();
    const reason = deadLetterReasonForState(to);

    // check-pg-drain: ok — full while loop exhausts all rows, natural drain
    var dead = try conn.query(
        \\UPDATE run_side_effects
        \\SET status = 'dead_letter',
        \\    details = CASE
        \\        WHEN details IS NULL OR details = '' THEN $3
        \\        ELSE details || ' | ' || $3
        \\    END,
        \\    updated_at = $2
        \\WHERE run_id = $1 AND status = 'claimed'
        \\RETURNING effect_key
    , .{ run_id, now_ms, reason });
    defer dead.deinit();

    var dead_lettered: u32 = 0;
    while (try dead.next()) |row| {
        const effect_key = try row.get([]u8, 0);
        try upsertSideEffectOutbox(
            conn,
            run_id,
            effect_key,
            .dead_letter,
            "reconciled_dead_letter",
            reason,
            to.label(),
            now_ms,
        );
        metrics.incOutboxDeadLetter();
        dead_lettered += 1;
    }

    return dead_lettered;
}

/// Allowed transitions (from → to). Encodes the state machine contract.
const ALLOWED = [_][2]types.RunState{
    .{ .SPEC_QUEUED, .RUN_PLANNED },
    .{ .RUN_PLANNED, .BLOCKED },
    .{ .RUN_PLANNED, .PATCH_IN_PROGRESS },
    .{ .PATCH_IN_PROGRESS, .BLOCKED },
    .{ .PATCH_IN_PROGRESS, .PATCH_READY },
    .{ .PATCH_READY, .BLOCKED },
    .{ .PATCH_READY, .VERIFICATION_IN_PROGRESS },
    .{ .VERIFICATION_IN_PROGRESS, .PR_PREPARED },
    .{ .VERIFICATION_IN_PROGRESS, .VERIFICATION_FAILED },
    .{ .VERIFICATION_IN_PROGRESS, .BLOCKED },
    .{ .VERIFICATION_FAILED, .PATCH_IN_PROGRESS },
    .{ .VERIFICATION_FAILED, .BLOCKED },
    .{ .PR_PREPARED, .PR_OPENED },
    .{ .PR_OPENED, .NOTIFIED },
    .{ .NOTIFIED, .DONE },
    .{ .BLOCKED, .NOTIFIED_BLOCKED },
    // M17_001 §3.3: operator cancel from any active state → CANCELLED (terminal, non-billable)
    .{ .SPEC_QUEUED, .CANCELLED },
    .{ .RUN_PLANNED, .CANCELLED },
    .{ .PATCH_IN_PROGRESS, .CANCELLED },
    .{ .PATCH_READY, .CANCELLED },
    .{ .VERIFICATION_IN_PROGRESS, .CANCELLED },
    .{ .VERIFICATION_FAILED, .CANCELLED },
    .{ .PR_PREPARED, .CANCELLED },
    // M21_001: abort transitions
    .{ .SPEC_QUEUED, .ABORTED },
    .{ .RUN_PLANNED, .ABORTED },
    .{ .PATCH_IN_PROGRESS, .ABORTED },
    .{ .PATCH_READY, .ABORTED },
    .{ .VERIFICATION_IN_PROGRESS, .ABORTED },
    .{ .VERIFICATION_FAILED, .ABORTED },
    .{ .PR_PREPARED, .ABORTED },
};

pub fn isAllowed(from: types.RunState, to: types.RunState) bool {
    for (ALLOWED) |pair| {
        if (pair[0] == from and pair[1] == to) return true;
    }
    return false;
}

/// Fetch the current state and attempt for a run from Postgres.
pub fn getRunState(
    conn: *pg.Conn,
    run_id: []const u8,
) !struct { state: types.RunState, attempt: u32 } {
    var result = try conn.query(
        \\SELECT state, attempt FROM runs WHERE run_id = $1
    , .{run_id});
    defer result.deinit();

    const row = try result.next() orelse return TransitionError.RunNotFound;
    const state_str = try row.get([]u8, 0);
    const attempt = @as(u32, @intCast(try row.get(i32, 1)));
    const state = try types.RunState.fromStr(state_str);
    try result.drain();
    return .{ .state = state, .attempt = attempt };
}

/// Apply a state transition, validate it, and append to run_transitions.
/// Returns the new attempt number.
pub fn transition(
    conn: *pg.Conn,
    run_id: []const u8,
    to: types.RunState,
    actor: types.Actor,
    reason_code: types.ReasonCode,
    notes: ?[]const u8,
) !u32 {
    const current = try getRunState(conn, run_id);

    if (current.state.isTerminal()) {
        return TransitionError.RunAlreadyTerminal;
    }

    if (!isAllowed(current.state, to)) {
        log.err("state.invalid_transition run_id={s} from={s} to={s}", .{
            run_id,
            current.state.label(),
            to.label(),
        });
        return TransitionError.InvalidTransition;
    }

    const now_ms = std.time.milliTimestamp();
    const attempt = current.attempt;

    // Append transition record
    {
        const transition_id = try id_format.generateTransitionId(conn._allocator);
        defer conn._allocator.free(transition_id);
        _ = try conn.exec(
            \\INSERT INTO run_transitions
            \\  (id, run_id, attempt, state_from, state_to, actor, reason_code, notes, ts)
            \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        , .{
            transition_id,
            run_id,
            @as(i32, @intCast(attempt)),
            current.state.label(),
            to.label(),
            actor.label(),
            reason_code.label(),
            notes,
            now_ms,
        });
    }

    // CAS update run state: fail if another worker moved the state first.
    {
        var r = try conn.query(
            \\UPDATE runs
            \\SET state = $1, updated_at = $2
            \\WHERE run_id = $3 AND state = $4
            \\RETURNING attempt
        , .{ to.label(), now_ms, run_id, current.state.label() });
        defer r.deinit();

        _ = try r.next() orelse {
            log.err("state.cas_transition_fail run_id={s} expected={s} to={s}", .{
                run_id,
                current.state.label(),
                to.label(),
            });
            return TransitionError.InvalidTransition;
        };
        try r.drain();
    }

    const dead_lettered = reconcileSideEffectsForRunState(conn, run_id, to) catch |err| blk: {
        obs_log.logWarnErr(.state, err, "state.side_effect_reconcile_fail run_id={s} to={s}", .{
            run_id,
            to.label(),
        });
        break :blk @as(u32, 0);
    };

    var request_id: []const u8 = "-";
    {
        var rq = conn.query("SELECT request_id FROM runs WHERE run_id = $1", .{run_id}) catch null;
        if (rq) |*q| {
            defer q.*.deinit();
            if ((q.*.next() catch null)) |rrow| {
                if (rrow.get(?[]u8, 0) catch null) |rid| {
                    if (rid.len > 0) request_id = rid;
                }
            }
        }
    }

    log.info("state.transition run_id={s} request_id={s} from={s} to={s} actor={s}", .{
        run_id,
        request_id,
        current.state.label(),
        to.label(),
        actor.label(),
    });
    var detail_buf: [160]u8 = undefined;
    const detail = std.fmt.bufPrint(
        &detail_buf,
        "request_id={s} from={s} to={s} actor={s} reason={s} dead_lettered={d}",
        .{ request_id, current.state.label(), to.label(), actor.label(), reason_code.label(), dead_lettered },
    ) catch "state_transition";
    events.emit("state_transition", run_id, detail);

    return attempt;
}

/// Increment attempt counter (for retry flows).
pub fn incrementAttempt(conn: *pg.Conn, run_id: []const u8) !u32 {
    const now_ms = std.time.milliTimestamp();
    var r = try conn.query(
        \\UPDATE runs SET attempt = attempt + 1, updated_at = $1
        \\WHERE run_id = $2
        \\RETURNING attempt
    , .{ now_ms, run_id });
    defer r.deinit();

    const row = try r.next() orelse return TransitionError.RunNotFound;
    const new_attempt = @as(u32, @intCast(try row.get(i32, 0)));
    try r.drain();
    return new_attempt;
}

/// Write usage ledger entry after an agent call completes.
pub fn writeUsage(
    conn: *pg.Conn,
    run_id: []const u8,
    attempt: u32,
    actor: types.Actor,
    token_count: u64,
    agent_seconds: u64,
) !void {
    const now_ms = std.time.milliTimestamp();
    const usage_id = try id_format.generateUsageLedgerId(conn._allocator);
    defer conn._allocator.free(usage_id);
    _ = try conn.exec(
        \\INSERT INTO usage_ledger
        \\  (id, run_id, attempt, actor, token_count, agent_seconds, created_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7)
    , .{
        usage_id,
        run_id,
        @as(i32, @intCast(attempt)),
        actor.label(),
        @as(i64, @intCast(token_count)),
        @as(i64, @intCast(agent_seconds)),
        now_ms,
    });
}

/// Register an artifact in the artifacts table.
pub fn registerArtifact(
    conn: *pg.Conn,
    run_id: []const u8,
    attempt: u32,
    artifact_name: []const u8,
    object_key: []const u8,
    checksum_sha256: []const u8,
    producer: types.Actor,
) !void {
    const now_ms = std.time.milliTimestamp();
    const artifact_id = try id_format.generateArtifactId(conn._allocator);
    defer conn._allocator.free(artifact_id);
    _ = try conn.exec(
        \\INSERT INTO artifacts
        \\  (id, run_id, attempt, artifact_name, object_key, checksum_sha256, producer, created_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        \\ON CONFLICT (run_id, attempt, artifact_name) DO UPDATE
        \\  SET object_key = EXCLUDED.object_key,
        \\      checksum_sha256 = EXCLUDED.checksum_sha256,
        \\      created_at = EXCLUDED.created_at
    , .{
        artifact_id,
        run_id,
        @as(i32, @intCast(attempt)),
        artifact_name,
        object_key,
        checksum_sha256,
        producer.label(),
        now_ms,
    });
}

/// Claim a side effect so it can run at-most-once per run/effect_key.
/// Returns true when this caller acquired the claim, false when it was already claimed.
pub fn claimSideEffect(
    conn: *pg.Conn,
    run_id: []const u8,
    effect_key: []const u8,
    details: ?[]const u8,
) !bool {
    const now_ms = std.time.milliTimestamp();
    const side_effect_id = try id_format.generateSideEffectId(conn._allocator);
    defer conn._allocator.free(side_effect_id);
    var insert_claim = try conn.query(
        \\INSERT INTO run_side_effects
        \\  (id, run_id, effect_key, status, details, created_at, updated_at)
        \\VALUES ($1, $2, $3, 'claimed', $4, $5, $5)
        \\ON CONFLICT (run_id, effect_key) DO NOTHING
        \\RETURNING id
    , .{ side_effect_id, run_id, effect_key, details, now_ms });
    defer insert_claim.deinit();
    if ((try insert_claim.next()) != null) {
        try insert_claim.drain();
        try upsertSideEffectOutbox(conn, run_id, effect_key, .pending, "claimed", details, null, now_ms);
        metrics.incOutboxEnqueued();
        return true;
    }
    try insert_claim.drain();

    var reclaim = try conn.query(
        \\UPDATE run_side_effects
        \\SET status = 'claimed',
        \\    details = COALESCE($3, details),
        \\    updated_at = $4
        \\WHERE run_id = $1 AND effect_key = $2 AND status = 'dead_letter'
        \\RETURNING id
    , .{ run_id, effect_key, details, now_ms });
    defer reclaim.deinit();
    if ((try reclaim.next()) != null) {
        try reclaim.drain();
        try upsertSideEffectOutbox(conn, run_id, effect_key, .pending, "reclaimed", details, null, now_ms);
        metrics.incOutboxEnqueued();
        return true;
    }
    try reclaim.drain();

    return false;
}

pub fn markSideEffectDone(
    conn: *pg.Conn,
    run_id: []const u8,
    effect_key: []const u8,
    details: ?[]const u8,
) !void {
    const now_ms = std.time.milliTimestamp();
    var r = try conn.query(
        \\UPDATE run_side_effects
        \\SET status = 'done', details = COALESCE($3, details), updated_at = $4
        \\WHERE run_id = $1 AND effect_key = $2 AND status != 'done'
        \\RETURNING id
    , .{ run_id, effect_key, details, now_ms });
    defer r.deinit();
    const did_update = (try r.next()) != null;
    try r.drain();
    if (did_update) {
        try upsertSideEffectOutbox(conn, run_id, effect_key, .delivered, "done", details, null, now_ms);
        metrics.incOutboxDelivered();
    }
}

test "isAllowed covers all configured transitions" {
    const valid = [_][2]types.RunState{
        .{ .SPEC_QUEUED, .RUN_PLANNED },
        .{ .RUN_PLANNED, .BLOCKED },
        .{ .RUN_PLANNED, .PATCH_IN_PROGRESS },
        .{ .PATCH_IN_PROGRESS, .BLOCKED },
        .{ .PATCH_IN_PROGRESS, .PATCH_READY },
        .{ .PATCH_READY, .BLOCKED },
        .{ .PATCH_READY, .VERIFICATION_IN_PROGRESS },
        .{ .VERIFICATION_IN_PROGRESS, .PR_PREPARED },
        .{ .VERIFICATION_IN_PROGRESS, .VERIFICATION_FAILED },
        .{ .VERIFICATION_IN_PROGRESS, .BLOCKED },
        .{ .VERIFICATION_FAILED, .PATCH_IN_PROGRESS },
        .{ .VERIFICATION_FAILED, .BLOCKED },
        .{ .PR_PREPARED, .PR_OPENED },
        .{ .PR_OPENED, .NOTIFIED },
        .{ .NOTIFIED, .DONE },
        .{ .BLOCKED, .NOTIFIED_BLOCKED },
    };
    for (valid) |pair| {
        try std.testing.expect(isAllowed(pair[0], pair[1]));
    }
    // Invalid transitions must be rejected
    try std.testing.expect(!isAllowed(.DONE, .SPEC_QUEUED));
    try std.testing.expect(!isAllowed(.SPEC_QUEUED, .DONE));
}

// Reconciliation + outbox tests extracted to machine_reconcile_test.zig (500-line limit).
// M17_001 transition tests extracted to machine_m17_test.zig.
comptime {
    _ = @import("machine_m17_test.zig");
    _ = @import("machine_reconcile_test.zig");
}
