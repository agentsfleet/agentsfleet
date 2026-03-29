//! Per-tick reconcile logic and one-shot runner.
//!
//! Exports:
//!   - `reconcileTick` — run a single reconcile pass across all side-effect
//!                       subsystems (outbox, billing, proposals, posthog events).
//!   - `runOnce`       — call `reconcileTick`, emit result, return success bool.
//!
//! Ownership:
//!   - `reconcileTick` uses `std.heap.page_allocator` for sub-allocations that
//!     it frees before returning; it does not retain any slices.
//!   - `runOnce` borrows all parameters; result observation is fire-and-forget.

const std = @import("std");
const posthog = @import("posthog");
const db = @import("../../db/pool.zig");
const outbox = @import("../../state/outbox_reconciler.zig");
const billing_adapter = @import("../../state/billing_adapter.zig");
const billing_reconciler = @import("../../state/billing_reconciler.zig");
const proposals = @import("../../pipeline/scoring_mod/proposals.zig");
const posthog_events = @import("../../observability/posthog_events.zig");
const orphan_recovery = @import("../../state/orphan_recovery.zig");
const emit_mod = @import("emit.zig");

const log = std.log.scoped(.reconcile);

pub fn reconcileTick(pool: *db.Pool, posthog_client: ?*posthog.PostHogClient) !outbox.ReconcileResult {
    const conn = try pool.acquire();
    defer pool.release(conn);
    const side_effect_result = try outbox.reconcileStartup(conn);
    var adapter = try billing_adapter.adapterFromEnv(std.heap.page_allocator);
    defer adapter.deinit(std.heap.page_allocator);
    const billing_result = try billing_reconciler.reconcilePending(std.heap.page_allocator, conn, adapter, billing_reconciler.DEFAULT_BATCH_LIMIT);
    const proposal_result = try proposals.reconcilePendingProposalGenerations(conn, std.heap.page_allocator, 0);
    if (proposal_result.ready > 0 or proposal_result.rejected > 0) {
        log.info("reconcile.proposal_generation ready={d} rejected={d}", .{ proposal_result.ready, proposal_result.rejected });
    }
    const reconcile_now_ms = std.time.milliTimestamp();
    const auto_approval_result = try proposals.reconcileDueAutoApprovalProposals(conn, std.heap.page_allocator, 0, reconcile_now_ms);
    if (auto_approval_result.applied > 0 or auto_approval_result.config_changed > 0 or auto_approval_result.rejected > 0 or auto_approval_result.expired > 0) {
        log.info(
            "reconcile.proposal_auto_approval applied={d} config_changed={d} rejected={d} expired={d}",
            .{ auto_approval_result.applied, auto_approval_result.config_changed, auto_approval_result.rejected, auto_approval_result.expired },
        );
    }
    if (auto_approval_result.applied > 0) {
        const items = try proposals.listAppliedAutoProposalTelemetryAt(conn, std.heap.page_allocator, reconcile_now_ms);
        defer {
            for (items) |*item| item.deinit(std.heap.page_allocator);
            std.heap.page_allocator.free(items);
        }
        for (items) |item| {
            posthog_events.trackAgentHarnessChanged(
                posthog_client,
                posthog_events.distinctIdOrSystem("system:auto"),
                item.agent_id,
                item.proposal_id,
                item.workspace_id,
                item.approval_mode,
                item.trigger_reason,
                item.fields_changed,
            );
        }
    }

    // M14_001: Orphan run recovery — uses a per-tick arena to avoid
    // page_allocator mmap/munmap syscalls on the hot path.
    var orphan_arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer orphan_arena.deinit();
    const orphan_alloc = orphan_arena.allocator();
    const orphan_config = orphan_recovery.loadConfig(orphan_alloc);
    // NOTE: Redis client not yet plumbed to reconciler. Pass null — re-queue
    // falls back to blocking. To enable re-queue, thread a Redis client from
    // reconcile.zig → daemon.zig → tick.zig and pass it here.
    const orphan_result = orphan_recovery.recoverOrphanedRuns(
        orphan_alloc,
        conn,
        posthog_client,
        null, // queue: ?*redis_client.Client — not available in reconciler yet
        orphan_config,
    ) catch |err| blk: {
        log.warn("reconcile.orphan_recovery_fail err={s}", .{@errorName(err)});
        break :blk orphan_recovery.OrphanRecoveryResult{};
    };
    if (orphan_result.blocked > 0 or orphan_result.requeued > 0) {
        log.info("reconcile.orphan_recovery blocked={d} requeued={d}", .{
            orphan_result.blocked, orphan_result.requeued,
        });
    }

    return .{
        .dead_lettered = side_effect_result.dead_lettered + billing_result.dead_lettered,
        .skipped = side_effect_result.skipped,
    };
}

pub fn runOnce(alloc: std.mem.Allocator, pool: *db.Pool, posthog_client: ?*posthog.PostHogClient) bool {
    const start_ms = std.time.milliTimestamp();
    const result = reconcileTick(pool, posthog_client) catch |err| {
        emit_mod.emitResult(alloc, start_ms, null, err);
        return false;
    };

    emit_mod.emitResult(alloc, start_ms, result, null);
    return true;
}
