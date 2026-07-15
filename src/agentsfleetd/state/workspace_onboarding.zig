//! The derivable onboarding signals for a workspace, read in one round trip.
//!
//! This is the server side of the consolidation Indy asked for: instead of the
//! client firing five separate reads (fleets, secrets, events, steer, provider)
//! plus a preferences read, the onboarding handler reads all five derivable
//! signals here in ONE query and folds in the preferences bag — one HTTP call,
//! one connection, one auth, where there used to be six.

const std = @import("std");
const pg = @import("pg");

const PgQuery = @import("../db/pg_query.zig").PgQuery;
const sql = @import("workspace_onboarding/sql.zig");
const tenant_provider = @import("tenant_provider.zig");

// The `actor LIKE` pattern that marks a steer event. The `steer:` prefix is the
// event actor the messages handler stamps; the `%` makes it a prefix match the
// index can serve. Kept as a named constant (RULE UFS) and bound as a query
// parameter (RULE NSQ), never inlined into the SQL text.
pub const STEER_ACTOR_LIKE = "steer:%";

pub const Signals = struct {
    has_fleet: bool,
    has_secret: bool,
    has_processed_event: bool,
    has_steer_event: bool,
    model_configured: bool,
};

// Reads every derivable onboarding signal for a workspace. `tenant_id` is the
// caller's tenant (from the principal) — the model check is tenant-scoped, the
// rest workspace-scoped. `model_configured` is true when the tenant has its own
// non-empty model selection OR an active platform default exists (a fresh tenant
// rides the platform default), and false only when no model exists anywhere.
pub fn read(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    tenant_id: []const u8,
) !Signals {
    const row_vals = blk: {
        var q = PgQuery.from(try conn.query(sql.SELECT_SIGNALS, .{
            workspace_id,
            STEER_ACTOR_LIKE,
            tenant_id,
        }));
        defer q.deinit();
        const row = (try q.next()) orelse return error.RowMissing;
        break :blk .{
            .has_fleet = try row.get(bool, 0),
            .has_secret = try row.get(bool, 1),
            .has_event = try row.get(bool, 2),
            .has_steer = try row.get(bool, 3),
            .tenant_model = try row.get(bool, 4),
        };
    };

    // The query result is fully drained (defer above) before this second read on
    // the same connection — the platform-default view runs its own query.
    const model_configured = row_vals.tenant_model or try platformDefaultConfigured(alloc, conn);

    return .{
        .has_fleet = row_vals.has_fleet,
        .has_secret = row_vals.has_secret,
        .has_processed_event = row_vals.has_event,
        .has_steer_event = row_vals.has_steer,
        .model_configured = model_configured,
    };
}

// True when an active platform default resolves to a non-empty model. Reuses the
// provider resolver so "what counts as the platform default" is defined in one
// place, not re-derived here.
fn platformDefaultConfigured(alloc: std.mem.Allocator, conn: *pg.Conn) !bool {
    var view = (try tenant_provider.platformDefaultView(alloc, conn)) orelse return false;
    defer view.deinit(alloc);
    return std.mem.trim(u8, view.model, " ").len > 0;
}
