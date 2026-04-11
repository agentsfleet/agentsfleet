//! Per-tick reconcile logic and one-shot runner.
//!
//! Exports:
//!   - `reconcileTick` — run a single reconcile pass (outbox only post-M10_001).
//!   - `runOnce`       — call `reconcileTick`, emit result, return success bool.

const std = @import("std");
const posthog = @import("posthog");
const db = @import("../../db/pool.zig");
const outbox = @import("../../state/outbox_reconciler.zig");
const emit_mod = @import("emit.zig");

const log = std.log.scoped(.reconcile);

pub fn reconcileTick(pool: *db.Pool, posthog_client: ?*posthog.PostHogClient) !outbox.ReconcileResult {
    _ = posthog_client;
    const conn = try pool.acquire();
    defer pool.release(conn);
    return try outbox.reconcileStartup(conn);
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
