//! M28_001: Webhook sig lookup — resolves webhook_secret_ref and
//! trigger.token from core.zombies for the webhook_sig middleware.
//!
//! Extracted from serve.zig to stay under the 350-line gate (RULE FLL).

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const crypto_store = @import("../secrets/crypto_store.zig");
const auth_mw = @import("../auth/middleware/mod.zig");

const LookupResult = auth_mw.webhook_sig_mod.LookupResult;

const log = std.log.scoped(.webhook_sig_lookup);

/// Queries core.zombies for webhook_secret_ref and trigger.token,
/// then resolves the vault secret via crypto_store.load.
/// Context is `*pg.Pool` (type-safe, no anyopaque — RULE NTE).
pub fn lookup(
    pool: *pg.Pool,
    zombie_id: []const u8,
    alloc: std.mem.Allocator,
) anyerror!?LookupResult {
    const conn = try pool.acquire();
    defer pool.release(conn);

    var q = PgQuery.from(try conn.query(
        \\SELECT workspace_id::text,
        \\       config_json->'trigger'->>'token',
        \\       webhook_secret_ref
        \\FROM core.zombies WHERE id = $1::uuid
    , .{zombie_id}));
    defer q.deinit();
    const row = try q.next() orelse return null;

    const workspace_id = try row.get([]const u8, 0);
    const token_raw = row.get([]const u8, 1) catch null;
    const token: ?[]const u8 = if (token_raw) |t| try alloc.dupe(u8, t) else null;
    errdefer if (token) |t| alloc.free(t);

    const ref_raw = row.get([]const u8, 2) catch null;
    const secret: ?[]const u8 = blk: {
        const ref = ref_raw orelse break :blk null;
        const conn2 = try pool.acquire();
        defer pool.release(conn2);
        break :blk crypto_store.load(alloc, conn2, workspace_id, ref) catch |err| {
            log.err("vault_load_failed ref={s} err={s}", .{ ref, @errorName(err) });
            break :blk null;
        };
    };

    return .{ .expected_secret = secret, .expected_token = token };
}
