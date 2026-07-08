const std = @import("std");
const pg = @import("pg");

const base = @import("../db/test_fixtures.zig");
const id_format = @import("../types/id_format.zig");
const models = @import("tenant_model_entries.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const ALLOC = std.testing.allocator;
const TENANT_A = "0195b4ba-8d3a-7f13-8abc-ab1000000001";
const TENANT_B = "0195b4ba-8d3a-7f13-8abc-ab1000000002";
const WS_A = "0195b4ba-8d3a-7f13-8abc-ab2000000001";
const WS_A_SECONDARY = "0195b4ba-8d3a-7f13-8abc-ab2000000009";
const WS_B = "0195b4ba-8d3a-7f13-8abc-ab2000000002";
const SECRET_SHARED = "models-anthropic-main";
const SECRET_LOCAL = "models-local-qwen";
const MODEL_OPUS = "anthropic/claude-opus-4.1";
const MODEL_FABLE = "anthropic/fable-5";
const MODEL_QWEN = "qwen/qwen3-local";

test "test_tenant_model_entries_create_list_tenant_scoped" {
    const db = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db.pool.deinit();
    defer db.pool.release(db.conn);
    try seedTenants(db.conn);
    defer cleanup(db.conn);
    try seedSecret(db.conn, WS_A, SECRET_SHARED);
    try seedSecret(db.conn, WS_B, SECRET_SHARED);

    const a_id = try id_format.generateTenantModelEntryId(ALLOC);
    defer ALLOC.free(a_id);
    const b_id = try id_format.generateTenantModelEntryId(ALLOC);
    defer ALLOC.free(b_id);

    var a = try models.create(ALLOC, db.conn, .{
        .id = a_id,
        .tenant_id = TENANT_A,
        .model_id = MODEL_OPUS,
        .secret_ref = SECRET_SHARED,
    });
    defer a.deinit(ALLOC);
    var b = try models.create(ALLOC, db.conn, .{
        .id = b_id,
        .tenant_id = TENANT_B,
        .model_id = MODEL_FABLE,
        .secret_ref = SECRET_SHARED,
    });
    defer b.deinit(ALLOC);

    const rows = try models.list(ALLOC, db.conn, TENANT_A);
    defer models.deinitEntryList(rows, ALLOC);
    try std.testing.expectEqual(@as(usize, 1), rows.len);
    try std.testing.expectEqualStrings(TENANT_A, rows[0].tenant_id);
    try std.testing.expectEqualStrings(MODEL_OPUS, rows[0].model_id);
}

test "test_tenant_model_entries_duplicate_rejected" {
    const db = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db.pool.deinit();
    defer db.pool.release(db.conn);
    try seedTenants(db.conn);
    defer cleanup(db.conn);
    try seedSecret(db.conn, WS_A, SECRET_SHARED);

    const first_id = try id_format.generateTenantModelEntryId(ALLOC);
    defer ALLOC.free(first_id);
    var first = try models.create(ALLOC, db.conn, .{
        .id = first_id,
        .tenant_id = TENANT_A,
        .model_id = MODEL_OPUS,
        .secret_ref = SECRET_SHARED,
    });
    defer first.deinit(ALLOC);

    const duplicate_id = try id_format.generateTenantModelEntryId(ALLOC);
    defer ALLOC.free(duplicate_id);
    try std.testing.expectError(models.StateError.DuplicateEntry, models.create(ALLOC, db.conn, .{
        .id = duplicate_id,
        .tenant_id = TENANT_A,
        .model_id = MODEL_OPUS,
        .secret_ref = SECRET_SHARED,
    }));
}

test "test_tenant_model_entries_update_delete_leaves_secret" {
    const db = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db.pool.deinit();
    defer db.pool.release(db.conn);
    try seedTenants(db.conn);
    defer cleanup(db.conn);
    try seedSecret(db.conn, WS_A, SECRET_LOCAL);

    const entry_id = try id_format.generateTenantModelEntryId(ALLOC);
    defer ALLOC.free(entry_id);
    var created = try models.create(ALLOC, db.conn, .{
        .id = entry_id,
        .tenant_id = TENANT_A,
        .model_id = MODEL_QWEN,
        .secret_ref = SECRET_LOCAL,
    });
    defer created.deinit(ALLOC);

    var updated = try models.updateModel(ALLOC, db.conn, TENANT_A, created.id, MODEL_FABLE);
    defer updated.deinit(ALLOC);
    try std.testing.expectEqualStrings(MODEL_FABLE, updated.model_id);
    try std.testing.expect(try models.delete(db.conn, TENANT_A, created.id));
    try std.testing.expect(try secretExists(db.conn, WS_A, SECRET_LOCAL));
    try std.testing.expectEqual(@as(i64, 0), try models.referencedSecretCount(db.conn, TENANT_A, SECRET_LOCAL));
}

test "test_tenant_model_entries_secret_lookup_uses_primary_workspace" {
    const db = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db.pool.deinit();
    defer db.pool.release(db.conn);
    try seedTenants(db.conn);
    defer cleanup(db.conn);
    try seedSecret(db.conn, WS_A_SECONDARY, SECRET_LOCAL);

    try std.testing.expect(!try models.secretExistsForTenant(db.conn, TENANT_A, SECRET_LOCAL));
    try seedSecret(db.conn, WS_A, SECRET_LOCAL);
    try std.testing.expect(try models.secretExistsForTenant(db.conn, TENANT_A, SECRET_LOCAL));
}

fn seedTenants(conn: *pg.Conn) !void {
    try base.seedTenantById(conn, TENANT_A, "models-a");
    try base.seedTenantById(conn, TENANT_B, "models-b");
    try base.seedWorkspaceWithTenant(conn, WS_A, TENANT_A);
    try base.seedWorkspaceWithTenant(conn, WS_A_SECONDARY, TENANT_A);
    try base.seedWorkspaceWithTenant(conn, WS_B, TENANT_B);
}

fn seedSecret(conn: *pg.Conn, workspace_id: []const u8, key_name: []const u8) !void {
    const secret_id = try id_format.generateVaultSecretId(ALLOC);
    defer ALLOC.free(secret_id);

    _ = try conn.exec(
        \\INSERT INTO vault.secrets
        \\  (id, workspace_id, key_name, encrypted_dek, dek_nonce, dek_tag,
        \\   nonce, ciphertext, tag, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, '', '', '', '', '', '', 0, 0)
        \\ON CONFLICT (workspace_id, key_name) DO UPDATE SET updated_at = EXCLUDED.updated_at
    , .{ secret_id, workspace_id, key_name });
}

fn secretExists(conn: *pg.Conn, workspace_id: []const u8, key_name: []const u8) !bool {
    var q = PgQuery.from(try conn.query(
        \\SELECT 1 FROM vault.secrets
        \\WHERE workspace_id = $1::uuid AND key_name = $2
        \\LIMIT 1
    , .{ workspace_id, key_name }));
    defer q.deinit();
    return (try q.next()) != null;
}

fn cleanup(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.tenant_model_entries WHERE tenant_id IN ($1::uuid, $2::uuid)", .{ TENANT_A, TENANT_B }) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id IN ($1::uuid, $2::uuid, $3::uuid)", .{ WS_A, WS_A_SECONDARY, WS_B }) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    base.teardownWorkspace(conn, WS_A_SECONDARY);
    base.teardownWorkspace(conn, WS_A);
    base.teardownWorkspace(conn, WS_B);
    base.teardownTenantById(conn, TENANT_A);
    base.teardownTenantById(conn, TENANT_B);
}
