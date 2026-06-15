// Webhook-specific DB fixtures for integration tests. LIVE DB ONLY — never
// creates temp tables. All fixtures go through the real schema so the
// middleware + handler code under test sees production-shaped rows.
//
// Cleanup is explicit in the test body (not deferred) — matches rbac
// pattern where deferred cleanup leaks connections at pool.deinit.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const crypto_store = @import("../secrets/crypto_store.zig");
const crypto_primitives = @import("../secrets/crypto_primitives.zig");
const IGNORED_ERROR_FMT = "ignored: {s}";

/// Set `ENCRYPTION_MASTER_KEY` so `crypto_store.store/load` can operate.
/// Safe to call once per test. Value is a fixed test key — not a secret.
const S_WEBHOOK = "webhook";

pub fn setTestEncryptionKey() void {
    crypto_primitives.setTestKek();
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
///   {"name":"x","x-agentsfleet":{"triggers":[{"type":"webhook","source":"github"}]}}
pub fn insertZombie(
    conn: *pg.Conn,
    fx: Fixture,
    config_json: []const u8,
) !void {
    const now_ms = clock.nowMillis();

    // Clean any prior state first — rerun resilience.
    try cleanup(conn, fx);

    _ = try conn.exec(
        "INSERT INTO core.tenants (tenant_id, name, created_at, updated_at) VALUES ($1, 'webhook-e2e-test', $2, $2)",
        .{ fx.tenant_id, now_ms },
    );
    _ = try conn.exec(
        \\INSERT INTO core.workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3)
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
    try crypto_store.store(alloc, conn, workspace_id, key_name, plaintext);
}

/// Insert a workspace credential at `agent:<credential_name>` containing
/// `{"webhook_secret": "<plaintext>"}`. Used by webhook integration tests
/// where the resolver reads the credential via `vault.loadJson`.
pub fn insertWebhookCredential(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    credential_name: []const u8,
    webhook_secret: []const u8,
) !void {
    // Use the JSON stringifier (not raw string interpolation) so test secrets
    // containing `"`, `\`, or control chars don't corrupt the credential JSON
    // and silently confuse `vault.loadJson` at lookup time.
    const Payload = struct { webhook_secret: []const u8 };
    const json = try std.json.Stringify.valueAlloc(
        alloc,
        Payload{ .webhook_secret = webhook_secret },
        .{},
    );
    defer alloc.free(json);
    const key_name = try std.fmt.allocPrint(alloc, "agent:{s}", .{credential_name});
    defer alloc.free(key_name);
    try crypto_store.store(alloc, conn, workspace_id, key_name, json);
}

/// Delete all rows this test created. Idempotent.
pub fn cleanup(conn: *pg.Conn, fx: Fixture) !void {
    _ = conn.exec("DELETE FROM core.zombies WHERE id = $1::uuid", .{fx.zombie_id}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid", .{fx.workspace_id}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.workspaces WHERE workspace_id = $1::uuid", .{fx.workspace_id}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.tenants WHERE tenant_id = $1::uuid", .{fx.tenant_id}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
}

/// Convenience: build a trigger config JSON for a given source. Optionally
/// pins an explicit `credential_name` override (defaults to `source` at
/// resolve time). Caller owns returned slice.
pub fn buildTriggerConfig(
    alloc: std.mem.Allocator,
    source: []const u8,
    credential_name: ?[]const u8,
) ![]u8 {
    // Use the JSON stringifier (not `{s}` interpolation) so test inputs
    // containing `"` or `\` round-trip correctly — same fix applied to
    // `insertWebhookCredential` above.
    const TriggerWith = struct {
        type: []const u8 = S_WEBHOOK,
        source: []const u8,
        credential_name: []const u8,
    };
    const TriggerNoOverride = struct {
        type: []const u8 = S_WEBHOOK,
        source: []const u8,
    };
    const WrapWith = struct { @"x-agentsfleet": struct { triggers: [1]TriggerWith } };
    const WrapNoOverride = struct { @"x-agentsfleet": struct { triggers: [1]TriggerNoOverride } };
    if (credential_name) |name| {
        return std.json.Stringify.valueAlloc(
            alloc,
            WrapWith{ .@"x-agentsfleet" = .{ .triggers = .{.{ .source = source, .credential_name = name }} } },
            .{},
        );
    }
    return std.json.Stringify.valueAlloc(
        alloc,
        WrapNoOverride{ .@"x-agentsfleet" = .{ .triggers = .{.{ .source = source }} } },
        .{},
    );
}

/// Valid UUIDv7-shaped strings for fixture IDs. 15th char must be '7' per
/// schema CHECK constraint. These are test-only; collisions within a single
/// test are handled by `cleanup()` running at start of insertZombie.
pub const ID_TENANT_A = "0197a4ba-8d3a-7f13-8abc-11111111aa01";
pub const ID_WS_A = "0197a4ba-8d3a-7f13-8abc-11111111aa11";
pub const ID_AGENTSFLEET_A = "0197a4ba-8d3a-7f13-8abc-11111111aa21";
const ID_TENANT_B = "0197a4ba-8d3a-7f13-8abc-22222222bb01";

test "buildTriggerConfig with credential_name override produces valid JSON" {
    const alloc = std.testing.allocator;
    const got = try buildTriggerConfig(alloc, "github", "github-prod");
    defer alloc.free(got);
    const want = "{\"x-agentsfleet\":{\"triggers\":[{\"type\":\"webhook\",\"source\":\"github\",\"credential_name\":\"github-prod\"}]}}";
    try std.testing.expectEqualStrings(want, got);
}

test "buildTriggerConfig without override produces source-only config" {
    const alloc = std.testing.allocator;
    const got = try buildTriggerConfig(alloc, "github", null);
    defer alloc.free(got);
    const want = "{\"x-agentsfleet\":{\"triggers\":[{\"type\":\"webhook\",\"source\":\"github\"}]}}";
    try std.testing.expectEqualStrings(want, got);
}

test "fixture IDs match UUIDv7 constraint (15th char is 7)" {
    try std.testing.expectEqual(@as(u8, '7'), ID_TENANT_A[14]);
    try std.testing.expectEqual(@as(u8, '7'), ID_WS_A[14]);
    try std.testing.expectEqual(@as(u8, '7'), ID_AGENTSFLEET_A[14]);
    try std.testing.expectEqual(@as(u8, '7'), ID_TENANT_B[14]);
}
