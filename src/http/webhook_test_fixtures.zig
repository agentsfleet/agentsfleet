// Webhook-specific DB fixtures for integration tests. LIVE DB ONLY — never
// creates temp tables. All fixtures go through the real schema so the
// middleware + handler code under test sees production-shaped rows.
//
// Cleanup is explicit in the test body (not deferred) — matches byok/rbac
// pattern where deferred cleanup leaks connections at pool.deinit.

const std = @import("std");
const pg = @import("pg");
const crypto_store = @import("../secrets/crypto_store.zig");

const KEK_VERSION: u32 = 1;

/// Set `ENCRYPTION_MASTER_KEY_V1` so `crypto_store.store/load` can operate.
/// Safe to call once per test. Value is a fixed test key — not a secret.
pub fn setTestEncryptionKey() void {
    const c = @cImport(@cInclude("stdlib.h"));
    _ = c.setenv("ENCRYPTION_MASTER_KEY", "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20", 1);
}

pub const Fixture = struct {
    tenant_id: []const u8,
    workspace_id: []const u8,
    zombie_id: []const u8,
};

/// Insert tenant + workspace + zombie with the given trigger config JSON.
/// Caller must call `cleanup()` at end of test before `harness.deinit()`.
///
/// `config_json` is the ENTIRE config — e.g.:
///   {"name":"x","x-usezombie":{"trigger":{"type":"webhook","source":"github"}}}
pub fn insertZombie(
    conn: *pg.Conn,
    fx: Fixture,
    config_json: []const u8,
) !void {
    const now_ms = std.time.milliTimestamp();

    // Clean any prior state first — rerun resilience.
    try cleanup(conn, fx);

    _ = try conn.exec(
        "INSERT INTO tenants (tenant_id, name, created_at, updated_at) VALUES ($1, 'webhook-e2e-test', $2, $2)",
        .{ fx.tenant_id, now_ms },
    );
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'https://example.invalid/webhook-e2e', 'main', false, 1, $3, $3)
    , .{ fx.workspace_id, fx.tenant_id, now_ms });
    _ = try conn.exec(
        \\INSERT INTO core.zombies
        \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'webhook-e2e-zombie', '# test', '# test', $3::jsonb, 'active', $4, $4)
    , .{ fx.zombie_id, fx.workspace_id, config_json, now_ms });
}

/// Insert a vault secret that `crypto_store.load(workspace_id, key_name)` can retrieve.
/// Requires `setTestEncryptionKey()` to have been called.
pub fn insertVaultSecret(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
) !void {
    try crypto_store.store(alloc, conn, workspace_id, key_name, plaintext, KEK_VERSION);
}

/// Insert a workspace credential at `zombie:<credential_name>` containing
/// `{"webhook_secret": "<plaintext>"}`. Used by webhook integration tests
/// where the resolver reads the credential via `vault.loadJson`.
pub fn insertWebhookCredential(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    credential_name: []const u8,
    webhook_secret: []const u8,
) !void {
    const json = try std.fmt.allocPrint(
        alloc,
        "{{\"webhook_secret\":\"{s}\"}}",
        .{webhook_secret},
    );
    defer alloc.free(json);
    const key_name = try std.fmt.allocPrint(alloc, "zombie:{s}", .{credential_name});
    defer alloc.free(key_name);
    try crypto_store.store(alloc, conn, workspace_id, key_name, json, KEK_VERSION);
}

/// Delete all rows this test created. Idempotent.
pub fn cleanup(conn: *pg.Conn, fx: Fixture) !void {
    _ = conn.exec("DELETE FROM core.zombies WHERE id = $1::uuid", .{fx.zombie_id}) catch {};
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid", .{fx.workspace_id}) catch {};
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1::uuid", .{fx.workspace_id}) catch {};
    _ = conn.exec("DELETE FROM tenants WHERE tenant_id = $1::uuid", .{fx.tenant_id}) catch {};
}

/// Convenience: build a trigger config JSON for a given source. Optionally
/// pins an explicit `credential_name` override (defaults to `source` at
/// resolve time). Caller owns returned slice.
pub fn buildTriggerConfig(
    alloc: std.mem.Allocator,
    source: []const u8,
    credential_name: ?[]const u8,
) ![]u8 {
    if (credential_name) |name| {
        return std.fmt.allocPrint(
            alloc,
            "{{\"x-usezombie\":{{\"trigger\":{{\"type\":\"webhook\",\"source\":\"{s}\",\"credential_name\":\"{s}\"}}}}}}",
            .{ source, name },
        );
    }
    return std.fmt.allocPrint(
        alloc,
        "{{\"x-usezombie\":{{\"trigger\":{{\"type\":\"webhook\",\"source\":\"{s}\"}}}}}}",
        .{source},
    );
}

/// Valid UUIDv7-shaped strings for fixture IDs. 15th char must be '7' per
/// schema CHECK constraint. These are test-only; collisions within a single
/// test are handled by `cleanup()` running at start of insertZombie.
pub const ID_TENANT_A = "0197a4ba-8d3a-7f13-8abc-11111111aa01";
pub const ID_WS_A = "0197a4ba-8d3a-7f13-8abc-11111111aa11";
pub const ID_ZOMBIE_A = "0197a4ba-8d3a-7f13-8abc-11111111aa21";
const ID_TENANT_B = "0197a4ba-8d3a-7f13-8abc-22222222bb01";
const ID_WS_B = "0197a4ba-8d3a-7f13-8abc-22222222bb11";
const ID_ZOMBIE_B = "0197a4ba-8d3a-7f13-8abc-22222222bb21";

test "buildTriggerConfig with credential_name override produces valid JSON" {
    const alloc = std.testing.allocator;
    const got = try buildTriggerConfig(alloc, "github", "github-prod");
    defer alloc.free(got);
    const want = "{\"x-usezombie\":{\"trigger\":{\"type\":\"webhook\",\"source\":\"github\",\"credential_name\":\"github-prod\"}}}";
    try std.testing.expectEqualStrings(want, got);
}

test "buildTriggerConfig without override produces source-only config" {
    const alloc = std.testing.allocator;
    const got = try buildTriggerConfig(alloc, "github", null);
    defer alloc.free(got);
    const want = "{\"x-usezombie\":{\"trigger\":{\"type\":\"webhook\",\"source\":\"github\"}}}";
    try std.testing.expectEqualStrings(want, got);
}

test "fixture IDs match UUIDv7 constraint (15th char is 7)" {
    try std.testing.expectEqual(@as(u8, '7'), ID_TENANT_A[14]);
    try std.testing.expectEqual(@as(u8, '7'), ID_WS_A[14]);
    try std.testing.expectEqual(@as(u8, '7'), ID_ZOMBIE_A[14]);
    try std.testing.expectEqual(@as(u8, '7'), ID_TENANT_B[14]);
}
