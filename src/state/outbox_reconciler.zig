//! Startup outbox reconciler.
//! On worker boot, scans for stale `pending` side-effect outbox rows that
//! were never delivered (e.g. after a crash) and marks them `dead_letter`.
//! Uses SELECT FOR UPDATE SKIP LOCKED to avoid contention with concurrent workers.

const std = @import("std");
const pg = @import("pg");
const metrics = @import("../observability/metrics.zig");
const obs_log = @import("../observability/logging.zig");
const log = std.log.scoped(.outbox_reconciler);

pub const RECONCILE_BATCH_LIMIT: u32 = 64;

pub const ReconcileResult = struct {
    dead_lettered: u32,
    skipped: u32,
};

/// Reconcile stale pending outbox rows at startup using distributed batching.
///
/// Each batch grabs at most RECONCILE_BATCH_LIMIT rows using SKIP LOCKED,
/// so multiple workers starting concurrently each process disjoint batches
/// without flooding Postgres or starving each other. Each batch is its own
/// short transaction — no long-held locks.
///
/// Loops until a batch returns fewer than RECONCILE_BATCH_LIMIT rows,
/// meaning this worker's share is fully drained.
///
/// Idempotent: calling twice with no new pending rows returns zero.
pub fn reconcileStartup(conn: *pg.Conn) !ReconcileResult {
    var total_dead_lettered: u32 = 0;
    var batches: u32 = 0;

    while (true) {
        const batch_count = try reconcileBatch(conn);
        total_dead_lettered += batch_count;
        batches += 1;

        // Fewer than a full batch means we've drained everything
        // visible to us (other workers may handle the rest via SKIP LOCKED).
        if (batch_count < RECONCILE_BATCH_LIMIT) break;
    }

    if (total_dead_lettered > 0) {
        log.info("reconcile.startup_complete dead_lettered={d} batches={d}", .{ total_dead_lettered, batches });
    }

    return .{
        .dead_lettered = total_dead_lettered,
        .skipped = 0,
    };
}

/// Process one batch of up to RECONCILE_BATCH_LIMIT rows in a short transaction.
/// SKIP LOCKED ensures concurrent workers get disjoint row sets.
fn reconcileBatch(conn: *pg.Conn) !u32 {
    const now_ms = std.time.milliTimestamp();

    _ = try conn.exec("BEGIN", .{});
    var tx_open = true;
    errdefer if (tx_open) {
        _ = conn.exec("ROLLBACK", .{}) catch {};
    };

    // check-pg-drain: ok — full while loop exhausts all rows, natural drain
    var result = try conn.query(
        \\UPDATE run_side_effect_outbox
        \\SET status = 'dead_letter',
        \\    reconciled_state = 'startup_reconcile',
        \\    updated_at = $1
        \\WHERE id IN (
        \\    SELECT id FROM run_side_effect_outbox
        \\    WHERE status = 'pending'
        \\    ORDER BY created_at ASC
        \\    LIMIT $2
        \\    FOR UPDATE SKIP LOCKED
        \\)
        \\RETURNING run_id, effect_key
    , .{ now_ms, @as(i32, @intCast(RECONCILE_BATCH_LIMIT)) });
    defer result.deinit();

    var dead_lettered: u32 = 0;
    while (try result.next()) |_| {
        metrics.incOutboxDeadLetter();
        dead_lettered += 1;
    }

    _ = try conn.exec("COMMIT", .{});
    tx_open = false;

    return dead_lettered;
}

test "reconcileStartup result struct is zero-initializable" {
    const r = ReconcileResult{ .dead_lettered = 0, .skipped = 0 };
    try std.testing.expectEqual(@as(u32, 0), r.dead_lettered);
    try std.testing.expectEqual(@as(u32, 0), r.skipped);
}

test "RECONCILE_BATCH_LIMIT is 64" {
    try std.testing.expectEqual(@as(u32, 64), RECONCILE_BATCH_LIMIT);
}
