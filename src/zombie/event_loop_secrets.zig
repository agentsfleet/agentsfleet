//! Worker-side credential resolver for structured (JSON-object) credentials.
//!
//! Sits between `crypto_store` (KMS envelope) and the executor. The zombie
//! config carries a list of credential *names*; this module resolves each
//! one to a parsed JSON object suitable for `secrets_map` in
//! `executor.createExecution`, which the tool bridge consumes as
//! `${secrets.<name>.<field>}`.
//!
//! Pairs with `resolveFirstCredential` in `event_loop_helpers.zig`, which
//! is the legacy single-string path being phased out.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const error_codes = @import("../errors/error_registry.zig");
const vault = @import("../state/vault.zig");
const credential_key = @import("credential_key.zig");

const log = std.log.scoped(.zombie_event_loop);

pub const ResolvedSecret = struct {
    name: []const u8, // duped, owned by caller
    parsed: std.json.Parsed(std.json.Value), // caller calls .deinit()
};

/// Resolve every credential name to its parsed JSON object. Order is
/// preserved. Any missing name aborts with `error.CredentialNotFound`
/// (the agent loop surfaces this as `secret_not_found`).
///
/// On success the caller owns the slice and must call `freeResolved`
/// to release each entry's `name` dupe and `parsed.deinit()`. On error
/// any entries already resolved are released before returning.
pub fn resolveSecretsMap(
    alloc: Allocator,
    pool: *pg.Pool,
    workspace_id: []const u8,
    names: []const []const u8,
) ![]ResolvedSecret {
    var out: std.ArrayList(ResolvedSecret) = .{};
    errdefer freeBuilder(alloc, &out);

    const conn = try pool.acquire();
    defer pool.release(conn);

    for (names) |name| {
        const key_name = try credential_key.allocKeyName(alloc, name);
        defer alloc.free(key_name);

        const parsed = vault.loadJson(alloc, conn, workspace_id, key_name) catch |err| {
            if (err == error.NotFound) {
                log.warn(
                    "zombie_event_loop.credential_not_found workspace_id={s} name={s} error_code=" ++ error_codes.ERR_ZOMBIE_CREDENTIAL_MISSING,
                    .{ workspace_id, name },
                );
                return error.CredentialNotFound;
            }
            return err;
        };
        errdefer parsed.deinit();

        const name_dup = try alloc.dupe(u8, name);
        errdefer alloc.free(name_dup);

        try out.append(alloc, .{ .name = name_dup, .parsed = parsed });
    }
    return out.toOwnedSlice(alloc);
}

/// Release a slice returned by `resolveSecretsMap`.
pub fn freeResolved(alloc: Allocator, items: []ResolvedSecret) void {
    for (items) |it| {
        it.parsed.deinit();
        alloc.free(it.name);
    }
    alloc.free(items);
}

fn freeBuilder(alloc: Allocator, list: *std.ArrayList(ResolvedSecret)) void {
    for (list.items) |it| {
        it.parsed.deinit();
        alloc.free(it.name);
    }
    list.deinit(alloc);
}
