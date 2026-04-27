//! Shared types for UseZombie — state machine states, transitions,
//! run/spec IDs, event envelopes, and artifact metadata.

const std = @import("std");

// ── Run state machine ─────────────────────────────────────────────────────

const RunState = enum {
    SPEC_QUEUED,
    RUN_PLANNED,
    PATCH_IN_PROGRESS,
    PATCH_READY,
    VERIFICATION_IN_PROGRESS,
    VERIFICATION_FAILED,
    PR_PREPARED,
    PR_OPENED,
    NOTIFIED,
    DONE,
    BLOCKED,
    NOTIFIED_BLOCKED,
    /// M17_001 §3.3: operator-requested cancellation — distinct from BLOCKED;
    /// billing finalises as non-billable, scoring records outcome=cancelled.
    CANCELLED,
    /// M21_001 §5.4: abort via CLI /stop or UI — terminal.
    ABORTED,

    pub fn label(self: RunState) []const u8 {
        return switch (self) {
            .SPEC_QUEUED => "SPEC_QUEUED",
            .RUN_PLANNED => "RUN_PLANNED",
            .PATCH_IN_PROGRESS => "PATCH_IN_PROGRESS",
            .PATCH_READY => "PATCH_READY",
            .VERIFICATION_IN_PROGRESS => "VERIFICATION_IN_PROGRESS",
            .VERIFICATION_FAILED => "VERIFICATION_FAILED",
            .PR_PREPARED => "PR_PREPARED",
            .PR_OPENED => "PR_OPENED",
            .NOTIFIED => "NOTIFIED",
            .DONE => "DONE",
            .BLOCKED => "BLOCKED",
            .NOTIFIED_BLOCKED => "NOTIFIED_BLOCKED",
            .CANCELLED => "CANCELLED",
            .ABORTED => "ABORTED",
        };
    }

    fn fromStr(s: []const u8) !RunState {
        return std.meta.stringToEnum(RunState, s) orelse error.UnknownState;
    }

    /// Returns true if the run can be retried from this state.
    pub fn isRetryable(self: RunState) bool {
        return switch (self) {
            .VERIFICATION_FAILED, .BLOCKED, .NOTIFIED_BLOCKED => true,
            else => false,
        };
    }

    /// Returns true if the run is in a terminal state.
    pub fn isTerminal(self: RunState) bool {
        return switch (self) {
            .DONE, .NOTIFIED_BLOCKED, .CANCELLED, .ABORTED => true,
            else => false,
        };
    }
};

// ── Reason codes ──────────────────────────────────────────────────────────

pub const ReasonCode = enum {
    PLAN_COMPLETE,
    PATCH_STARTED,
    PATCH_COMMITTED,
    VALIDATION_PASSED,
    VALIDATION_FAILED,
    RETRIES_EXHAUSTED,
    PR_CREATED,
    NOTIFICATION_SENT,
    MANUAL_RETRY,
    WORKSPACE_PAUSED,
    AGENT_TIMEOUT,
    AGENT_CRASH,
    AUTH_FAILED,
    RATE_LIMITED,
    MISSING_TESTS,
    SPEC_MISMATCH,
    WORKER_CRASH_ORPHAN,
    ORPHAN_REQUEUED,
    // M17_001 §1.2 / §3 — budget and cancellation reasons
    TOKEN_BUDGET_EXCEEDED,
    WALL_TIME_EXCEEDED,
    REPAIR_LOOPS_EXHAUSTED,
    RUN_CANCELLED,
    // M21_001
    INTERRUPT_DELIVERED,
    INTERRUPT_QUEUED,
    RUN_ABORTED,

    pub fn label(self: ReasonCode) []const u8 {
        return @tagName(self);
    }
};

// ── Actor roles ───────────────────────────────────────────────────────────

const Actor = enum {
    echo,
    scout,
    warden,
    orchestrator,

    pub fn label(self: Actor) []const u8 {
        return @tagName(self);
    }
};

const TrustLevel = enum {
    unearned,
    trusted,

    pub fn label(self: TrustLevel) []const u8 {
        return switch (self) {
            .unearned => "UNEARNED",
            .trusted => "TRUSTED",
        };
    }
};

// ── Run ingress mode ──────────────────────────────────────────────────────

const IngressMode = enum {
    web,
    api,

    fn fromStr(s: []const u8) !IngressMode {
        return std.meta.stringToEnum(IngressMode, s) orelse error.UnknownMode;
    }
};

// ── Run record ────────────────────────────────────────────────────────────

pub const Run = struct {
    run_id: []const u8,
    workspace_id: []const u8,
    spec_id: []const u8,
    tenant_id: []const u8,
    state: RunState,
    attempt: u32,
    mode: IngressMode,
    requested_by: []const u8,
    idempotency_key: []const u8,
    branch: []const u8,
    pr_url: ?[]const u8,
    created_at: i64, // Unix ms
    updated_at: i64,
};

// ── Spec record ───────────────────────────────────────────────────────────

const SpecStatus = enum {
    pending,
    in_progress,
    done,
    failed,

    fn fromStr(s: []const u8) !SpecStatus {
        return std.meta.stringToEnum(SpecStatus, s) orelse error.UnknownSpecStatus;
    }
};

pub const Spec = struct {
    spec_id: []const u8,
    workspace_id: []const u8,
    tenant_id: []const u8,
    file_path: []const u8, // relative path in repo (e.g. "docs/spec/PENDING_001.md")
    title: []const u8,
    status: SpecStatus,
    created_at: i64,
    updated_at: i64,
};

// ── Transition record ─────────────────────────────────────────────────────

pub const Transition = struct {
    id: i64,
    run_id: []const u8,
    attempt: u32,
    state_from: RunState,
    state_to: RunState,
    actor: Actor,
    reason_code: ReasonCode,
    notes: ?[]const u8,
    ts: i64, // Unix ms
};

// ── Usage ledger ──────────────────────────────────────────────────────────

const UsageLedgerEntry = struct {
    run_id: []const u8,
    attempt: u32,
    actor: Actor,
    token_count: u64,
    agent_seconds: u64,
    created_at: i64,
};

// ── Artifact ──────────────────────────────────────────────────────────────

const ArtifactName = enum {
    plan_json,
    implementation_md,
    validation_md,
    defects_md,
    run_summary_md,

    fn filename(self: ArtifactName, attempt: u32, alloc: std.mem.Allocator) ![]const u8 {
        return switch (self) {
            .plan_json => try alloc.dupe(u8, "plan.json"),
            .implementation_md => try alloc.dupe(u8, "implementation.md"),
            .validation_md => try alloc.dupe(u8, "validation.md"),
            .defects_md => try std.fmt.allocPrint(alloc, "attempt_{d}_defects.md", .{attempt}),
            .run_summary_md => try alloc.dupe(u8, "run_summary.md"),
        };
    }
};

const Artifact = struct {
    run_id: []const u8,
    attempt: u32,
    artifact_name: []const u8,
    object_key: []const u8, // git path: docs/runs/<run_id>/<artifact_name>
    checksum_sha256: []const u8,
    producer: Actor,
    created_at: i64,
};

// ── Workspace ─────────────────────────────────────────────────────────────

pub const Workspace = struct {
    workspace_id: []const u8,
    tenant_id: []const u8,
    repo_url: []const u8,
    default_branch: []const u8,
    paused: bool,
    paused_reason: ?[]const u8,
    version: i64,
    created_at: i64,
    updated_at: i64,
};

// ── Policy event ──────────────────────────────────────────────────────────

pub const PolicyDecision = enum { allow, deny, require_confirmation };

pub const ActionClass = enum { safe, sensitive, critical };

const PolicyEvent = struct {
    run_id: ?[]const u8,
    workspace_id: []const u8,
    action_class: ActionClass,
    decision: PolicyDecision,
    rule_id: []const u8,
    actor: []const u8,
    ts: i64,
};

// ── Workspace memory ──────────────────────────────────────────────────────

const WorkspaceMemory = struct {
    id: i64,
    workspace_id: []const u8,
    run_id: []const u8,
    content: []const u8,
    tags: []const u8, // JSON array as string
    created_at: i64,
    expires_at: ?i64,
};

// ── Event envelope ────────────────────────────────────────────────────────

pub const EventType = enum {
    transition,
    policy_decision,
    validation_result,
    cost_snapshot,
    notification_sent,
    nullclaw_run,
    tool_execution,
};

pub const Event = struct {
    event_id: []const u8,
    timestamp: []const u8, // RFC3339
    tenant_id: []const u8,
    workspace_id: []const u8,
    run_id: ?[]const u8,
    attempt: u32,
    actor: []const u8,
    event_type: EventType,
    // Optional fields (populated depending on event_type)
    state_from: ?[]const u8 = null,
    state_to: ?[]const u8 = null,
    reason_code: ?[]const u8 = null,
    cost_tokens: ?u64 = null,
    cost_runtime_seconds: ?u64 = null,
};

test {
    // Verify state transitions compile
    const s = RunState.SPEC_QUEUED;
    try std.testing.expectEqualStrings("SPEC_QUEUED", s.label());
    try std.testing.expect(!s.isTerminal());
    try std.testing.expect(!s.isRetryable());

    const done = RunState.DONE;
    try std.testing.expect(done.isTerminal());
}

// ── M17_001 — CANCELLED state and new reason codes ───────────────────────

// T1: CANCELLED label is exactly "CANCELLED" (no typo, no case variation).
test "M17: RunState.CANCELLED label is CANCELLED" {
    try std.testing.expectEqualStrings("CANCELLED", RunState.CANCELLED.label());
}

// T1: CANCELLED is terminal — billing finalises as non-billable.
test "M17: RunState.CANCELLED isTerminal returns true" {
    try std.testing.expect(RunState.CANCELLED.isTerminal());
}

// T2: CANCELLED must NOT be retryable — once cancelled, no retry path.
test "M17: RunState.CANCELLED isRetryable returns false" {
    try std.testing.expect(!RunState.CANCELLED.isRetryable());
}

// T2: DONE and NOTIFIED_BLOCKED remain terminal to catch regressions.
test "M17: existing terminal states still terminal after CANCELLED added" {
    try std.testing.expect(RunState.DONE.isTerminal());
    try std.testing.expect(RunState.NOTIFIED_BLOCKED.isTerminal());
}

// T2: Active states must NOT be terminal (regression guard).
test "M17: active states are not terminal" {
    const active = [_]RunState{
        .SPEC_QUEUED, .RUN_PLANNED,              .PATCH_IN_PROGRESS,
        .PATCH_READY, .VERIFICATION_IN_PROGRESS, .VERIFICATION_FAILED,
        .PR_PREPARED, .PR_OPENED,                .NOTIFIED,
        .BLOCKED,
    };
    for (active) |st| {
        try std.testing.expect(!st.isTerminal());
    }
}

// T1: fromStr round-trips "CANCELLED" back to the correct enum value.
test "M17: RunState fromStr CANCELLED round-trips" {
    const state = try RunState.fromStr("CANCELLED");
    try std.testing.expectEqual(RunState.CANCELLED, state);
}

// T3: fromStr returns error on unknown state string.
test "M17: RunState fromStr rejects unknown string" {
    try std.testing.expectError(error.UnknownState, RunState.fromStr("DELETED"));
    try std.testing.expectError(error.UnknownState, RunState.fromStr("cancelled"));
    try std.testing.expectError(error.UnknownState, RunState.fromStr(""));
}

// T1: new M17_001 ReasonCode labels are stable (used in audit logs / DB).
test "M17: ReasonCode M17 labels are stable" {
    try std.testing.expectEqualStrings("TOKEN_BUDGET_EXCEEDED", ReasonCode.TOKEN_BUDGET_EXCEEDED.label());
    try std.testing.expectEqualStrings("WALL_TIME_EXCEEDED", ReasonCode.WALL_TIME_EXCEEDED.label());
    try std.testing.expectEqualStrings("REPAIR_LOOPS_EXHAUSTED", ReasonCode.REPAIR_LOOPS_EXHAUSTED.label());
    try std.testing.expectEqualStrings("RUN_CANCELLED", ReasonCode.RUN_CANCELLED.label());
}

// T2: all M17 reason codes are distinct from each other and from existing codes.
test "M17: ReasonCode M17 values are distinct" {
    const m17 = [_]ReasonCode{
        .TOKEN_BUDGET_EXCEEDED,  .WALL_TIME_EXCEEDED,
        .REPAIR_LOOPS_EXHAUSTED, .RUN_CANCELLED,
    };
    for (m17, 0..) |a, i| {
        for (m17, 0..) |b, j| {
            if (i != j) {
                try std.testing.expect(a != b);
            }
        }
    }
}

// T4: BLOCKED is retryable (existing invariant must survive M17 additions).
test "M17: BLOCKED and VERIFICATION_FAILED remain retryable" {
    try std.testing.expect(RunState.BLOCKED.isRetryable());
    try std.testing.expect(RunState.VERIFICATION_FAILED.isRetryable());
    try std.testing.expect(RunState.NOTIFIED_BLOCKED.isRetryable());
}
