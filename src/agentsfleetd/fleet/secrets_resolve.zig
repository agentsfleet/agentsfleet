//! Credential resolver for structured (JSON-object) credentials.
//!
//! Sits between `crypto_store` (KMS envelope) and the runner engine. The fleet
//! config carries a list of credential *names*; this module resolves each one
//! to a parsed JSON object the lease verb hands back as `secrets_map` in the
//! `ExecutionPolicy`, which the runner's tool bridge consumes as
//! `${secrets.<name>.<field>}`.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const error_codes = @import("../errors/error_registry.zig");
const vault = @import("../state/vault.zig");
const integration = @import("../credentials/integration.zig");
const logging = @import("log");

const log = logging.scoped(.fleet_event_loop);

pub const ResolvedSecret = struct {
    name: []const u8, // duped, owned by caller
    parsed: std.json.Parsed(std.json.Value), // caller calls .deinit()
};

/// Resolve every credential name to its parsed JSON object. Order is
/// preserved. Any missing name aborts with `error.CredentialNotFound`
/// (the fleet loop surfaces this as `secret_not_found`).
///
/// On success the caller owns the slice — call `freeResolved`, or hand the
/// allocation to a request arena (the fleet service path's choice)
/// to release each entry's `name` dupe and `parsed.deinit()`. On error
/// any entries already resolved are released before returning.
pub fn resolveSecretsMap(
    alloc: Allocator,
    pool: *pg.Pool,
    workspace_id: []const u8,
    names: []const []const u8,
) ![]ResolvedSecret {
    var out: std.ArrayList(ResolvedSecret) = .empty;
    errdefer freeBuilder(alloc, &out);

    const conn = try pool.acquire();
    defer pool.release(conn);

    for (names) |name| {
        const parsed = vault.loadJson(alloc, conn, workspace_id, name) catch |err| {
            if (err == error.NotFound) {
                log.warn(
                    "credential_not_found",
                    .{ .workspace_id = workspace_id, .name = name, .error_code = error_codes.ERR_AGENTSFLEET_CREDENTIAL_MISSING },
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

/// Classify a resolved vault handle as ON-DEMAND mintable (returns its integration
/// id) or static (returns null — the lease ships the stored value as today).
///
/// The lease path calls this to split each resolved credential: a mintable one is
/// emitted into the typed, out-of-band `ExecutionPolicy.mintable` list (id-only —
/// the stored handle/App config NEVER reaches the child, Invariant 1/VLT); a
/// static one keeps its stored value in `secrets_map`. Reads the broker's
/// vault-handle integration field; an absent field (legacy credential) or an
/// unknown/unregistered id falls through as static (fail safe — never a mint).
pub fn mintableId(handle: std.json.Value) ?integration.Id {
    const obj = switch (handle) {
        .object => |o| o,
        else => return null,
    };
    const v = obj.get(integration.FIELD_INTEGRATION) orelse return null;
    const s = switch (v) {
        .string => |str| str,
        else => return null,
    };
    const id = integration.idFromString(s) orelse return null;
    return if (integration.mintsOnDemand(integration.REGISTRY, id)) id else null;
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

// ── Tests ────────────────────────────────────────────────────────────────────
// `mintableId` is pure (no DB) — the static-vs-mintable classification the lease
// path applies to route each credential to `secrets_map` vs the `mintable` list.

const testing = std.testing;

fn parseValue(arena: Allocator, json: []const u8) !std.json.Value {
    return std.json.parseFromSliceLeaky(std.json.Value, arena, json, .{});
}

test "test_runner_facing_classify" {
    var arena_state = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    // A github handle (on-demand integration) classifies as mintable → its id.
    const gh = try parseValue(arena, "{\"integration\":\"github\",\"installation_id\":\"42\",\"app_id\":\"7\"}");
    try testing.expectEqual(integration.Id.github, mintableId(gh).?);

    // A legacy credential (no integration field) is static — the resolve-as-today
    // path (Dimension 4.3); it stays in secrets_map with its stored value.
    const legacy = try parseValue(arena, "{\"api_token\":\"FlyTokenXyz\"}");
    try testing.expect(mintableId(legacy) == null);

    // `static` is registered but on_demand=false → static (stored token usable inline).
    const static_handle = try parseValue(arena, "{\"integration\":\"static\",\"token\":\"ghp_abc\"}");
    try testing.expect(mintableId(static_handle) == null);

    // A refresh-token integration (zoho/jira/linear) is on-demand → mintable.
    const zoho = try parseValue(arena, "{\"integration\":\"zoho\",\"refresh_token\":\"rt\"}");
    try testing.expectEqual(integration.Id.zoho, mintableId(zoho).?);

    // An unknown/unregistered id falls through as static (fail safe — never a mint).
    // api_key connectors (datadog/grafana/fly) are used directly, never minted.
    const unknown = try parseValue(arena, "{\"integration\":\"datadog\",\"token\":\"z\"}");
    try testing.expect(mintableId(unknown) == null);
}
