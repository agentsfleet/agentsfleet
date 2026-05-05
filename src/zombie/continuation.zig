//! M41 §7 continuation decision logic.
//!
//! When a stage finishes with `exit_ok=false` AND a `checkpoint_id`, the
//! agent is signalling "I voluntarily stopped to be resumed in a fresh
//! stage" (L3 chunk-threshold trigger). The worker is responsible for
//! re-enqueueing the same event chain as a synthetic continuation event
//! with `actor=continuation:<original_actor>` so the next stage opens
//! with a fresh context window and a `memory_recall` of the snapshot.
//! Domain-neutral: works for any zombie shape (incident response,
//! morning health check, cron-driven housekeeping, steer chats).
//!
//! Two safety properties live here, separately from the call site that
//! actually performs the XADD + PG count:
//!
//!   1. The **classifier** maps `(stage_result, prior_continuation_count)`
//!      onto a `Verdict`. Force-stop fires at exactly 10 prior
//!      continuations on the same chain — Invariant 4 of the spec.
//!      The classifier never touches Redis or Postgres.
//!
//!   2. The **payload builder** produces the JSON-stringified
//!      `request_json` body for the synthetic event. The shipped
//!      `core.zombie_events` schema (slot 018) carries `request_json`
//!      and `resumes_event_id` as the canonical columns; the payload
//!      respects the spec text in §7 step 2.
//!
//! Call-site integration (worker's processEvent) is a small follow-up:
//! after every stage, query `SELECT count(*) FROM core.zombie_events
//! WHERE zombie_id = $1 AND resumes_event_id = $2 AND event_type =
//! 'continuation'`, call `classify`, and act on the verdict.

const std = @import("std");
const executor_client = @import("../executor/client.zig");
const event_envelope = @import("event_envelope.zig");

/// Hard cap from Invariant 4: never re-enqueue a continuation if the
/// incident already produced this many. The 11th attempt force-stops
/// with a clear operator-facing error.
pub const max_continuations_per_chain: u32 = 10;

/// Outcome of `classify`. `enqueue` carries the bytes that the call site
/// XADDs onto `zombie:{id}:events`; `force_stop` carries the failure
/// label the call site UPDATEs onto the originating row.
pub const Verdict = union(enum) {
    /// Stage finished normally OR failed without a checkpoint. Worker
    /// continues its usual close-out path — no re-enqueue.
    no_continuation,
    /// Re-enqueue. `actor` is `continuation:<original_actor>` (flat —
    /// `event_envelope.buildContinuationActor` is idempotent on
    /// already-continuation actors). `original_event_id` is the chain
    /// link the new row's `resumes_event_id` points back at.
    enqueue: struct {
        actor: []const u8,
        original_event_id: []const u8,
        checkpoint_id: []const u8,
    },
    /// 11th continuation on the same chain. Worker UPDATEs the
    /// originating row with `failure_label = "chunk_chain_escalate_human"`,
    /// surfaces to operator, never XADDs.
    force_stop: struct {
        prior_continuation_count: u32,
    },
};

/// Decide whether to re-enqueue based on the finished stage and the
/// per-incident continuation count from `core.zombie_events`. Pure —
/// caller passes the count, this fn never reads from Postgres.
pub fn classify(
    stage: executor_client.ExecutorClient.StageResult,
    original_actor: []const u8,
    original_event_id: []const u8,
    prior_continuation_count: u32,
) Verdict {
    if (stage.exit_ok) return .no_continuation;
    const checkpoint_id = stage.checkpoint_id orelse return .no_continuation;
    if (checkpoint_id.len == 0) return .no_continuation;

    if (prior_continuation_count >= max_continuations_per_chain) {
        return .{ .force_stop = .{ .prior_continuation_count = prior_continuation_count } };
    }

    return .{ .enqueue = .{
        .actor = original_actor,
        .original_event_id = original_event_id,
        .checkpoint_id = checkpoint_id,
    } };
}

/// Stringify the `request_json` body for the synthetic continuation
/// event. Caller owns the returned slice. Shape per §7 step 2:
/// `{checkpoint_id, original_event_id}`.
pub fn buildContinuationRequestJson(
    alloc: std.mem.Allocator,
    checkpoint_id: []const u8,
    original_event_id: []const u8,
) ![]u8 {
    const Body = struct {
        checkpoint_id: []const u8,
        original_event_id: []const u8,
    };
    return std.json.Stringify.valueAlloc(alloc, Body{
        .checkpoint_id = checkpoint_id,
        .original_event_id = original_event_id,
    }, .{});
}

// ── Tests ──────────────────────────────────────────────────────────────────

const StageResult = executor_client.ExecutorClient.StageResult;

fn okStage() StageResult {
    return .{
        .content = "fine",
        .token_count = 100,
        .wall_seconds = 1,
        .exit_ok = true,
        .failure = null,
    };
}

fn chunkStage(checkpoint: ?[]const u8) StageResult {
    return .{
        .content = "needs continuation",
        .token_count = 200,
        .wall_seconds = 2,
        .exit_ok = false,
        .failure = null,
        .checkpoint_id = checkpoint,
    };
}

test "classify: exit_ok=true → no continuation regardless of checkpoint" {
    const stage = okStage();
    try std.testing.expectEqual(Verdict.no_continuation, classify(stage, "steer:k", "1729-0", 0));
    // Even with a stray checkpoint_id from a buggy executor, exit_ok
    // wins — we never re-enqueue a successful stage.
    var stage_with_cp = stage;
    stage_with_cp.checkpoint_id = "abc";
    try std.testing.expectEqual(Verdict.no_continuation, classify(stage_with_cp, "steer:k", "1729-0", 0));
}

test "classify: exit_ok=false without checkpoint_id → no continuation (real failure)" {
    const stage = chunkStage(null);
    try std.testing.expectEqual(Verdict.no_continuation, classify(stage, "steer:k", "1729-0", 0));
}

test "classify: empty checkpoint_id treated as no checkpoint" {
    const stage = chunkStage("");
    try std.testing.expectEqual(Verdict.no_continuation, classify(stage, "steer:k", "1729-0", 0));
}

test "classify: exit_ok=false + checkpoint_id + count<10 → enqueue" {
    const stage = chunkStage("ckpt-abc");
    const verdict = classify(stage, "steer:kishore", "1729874000000-0", 3);
    switch (verdict) {
        .enqueue => |e| {
            try std.testing.expectEqualStrings("steer:kishore", e.actor);
            try std.testing.expectEqualStrings("1729874000000-0", e.original_event_id);
            try std.testing.expectEqualStrings("ckpt-abc", e.checkpoint_id);
        },
        else => try std.testing.expect(false),
    }
}

test "classify: count == max_continuations_per_chain → force_stop on 11th attempt" {
    const stage = chunkStage("ckpt-xyz");
    const verdict = classify(stage, "webhook:github", "1729-0", max_continuations_per_chain);
    switch (verdict) {
        .force_stop => |s| try std.testing.expectEqual(@as(u32, max_continuations_per_chain), s.prior_continuation_count),
        else => try std.testing.expect(false),
    }
}

test "classify: count exceeds cap (defensive) → force_stop" {
    const stage = chunkStage("ckpt");
    const verdict = classify(stage, "cron:5min", "1729-0", 25);
    try std.testing.expect(verdict == .force_stop);
}

test "buildContinuationRequestJson produces compact JSON with both fields" {
    const out = try buildContinuationRequestJson(std.testing.allocator, "ckpt-abc", "1729874000000-0");
    defer std.testing.allocator.free(out);
    // We don't pin field ordering (zig std.json doesn't guarantee it),
    // but both keys + values must be present.
    try std.testing.expect(std.mem.indexOf(u8, out, "\"checkpoint_id\":\"ckpt-abc\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, out, "\"original_event_id\":\"1729874000000-0\"") != null);
}

