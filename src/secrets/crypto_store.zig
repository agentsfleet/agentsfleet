//! Database-backed secret storage using envelope encryption.
//! Depends on crypto_primitives for all crypto operations.

const std = @import("std");
const pg = @import("pg");
const id_format = @import("../types/id_format.zig");
const cp = @import("crypto_primitives.zig");

const log = std.log.scoped(.secrets);

const KEY_LEN = cp.KEY_LEN;
const NONCE_LEN = cp.NONCE_LEN;
const TAG_LEN = cp.TAG_LEN;

/// Store encrypted secret in vault.secrets with envelope encryption.
/// kek_version selects which ENCRYPTION_MASTER_KEY_V{N} env var to use.
pub fn store(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
    kek_version: u32,
) !void {
    const kek = try cp.loadKekByVersion(alloc, kek_version);

    var dek: [KEY_LEN]u8 = undefined;
    std.crypto.random.bytes(&dek);

    const wrapped_dek = try cp.encrypt(alloc, dek[0..], &kek);
    defer wrapped_dek.deinit(alloc);

    const encrypted_payload = try cp.encrypt(alloc, plaintext, &dek);
    defer encrypted_payload.deinit(alloc);

    const now_ms = std.time.milliTimestamp();

    const secret_id = try id_format.generateVaultSecretId(alloc);
    defer alloc.free(secret_id);
    _ = try conn.exec(
        \\INSERT INTO vault.secrets
        \\  (id, workspace_id, key_name, kek_version, encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
        \\ON CONFLICT (workspace_id, key_name) DO UPDATE
        \\SET kek_version = EXCLUDED.kek_version,
        \\    encrypted_dek = EXCLUDED.encrypted_dek,
        \\    dek_nonce = EXCLUDED.dek_nonce,
        \\    dek_tag = EXCLUDED.dek_tag,
        \\    nonce = EXCLUDED.nonce,
        \\    ciphertext = EXCLUDED.ciphertext,
        \\    tag = EXCLUDED.tag,
        \\    updated_at = EXCLUDED.updated_at
    , .{
        secret_id,
        workspace_id,
        key_name,
        @as(i32, @intCast(kek_version)),
        wrapped_dek.ciphertext,
        wrapped_dek.nonce[0..],
        wrapped_dek.tag[0..],
        encrypted_payload.nonce[0..],
        encrypted_payload.ciphertext,
        encrypted_payload.tag[0..],
        now_ms,
    });
    log.info("secret.stored workspace_id={s} key_name={s}", .{ workspace_id, key_name });
}

/// Load and decrypt a secret from vault.secrets.
/// Reads kek_version from the stored row and selects the correct KEK automatically.
pub fn load(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
) ![]u8 {
    var result = try conn.query(
        \\SELECT kek_version, encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag
        \\FROM vault.secrets
        \\WHERE workspace_id = $1 AND key_name = $2
    , .{ workspace_id, key_name });
    defer result.deinit();

    const row = try result.next() orelse {
        log.err("secret.not_found workspace_id={s} key_name={s} error_code=UZ-INTERNAL-002", .{ workspace_id, key_name });
        return cp.SecretError.NotFound;
    };

    const kek_version = @as(u32, @intCast(try row.get(i32, 0)));
    const encrypted_dek = try row.get([]u8, 1);
    const dek_nonce_slice = try row.get([]u8, 2);
    const dek_tag_slice = try row.get([]u8, 3);
    const payload_nonce_slice = try row.get([]u8, 4);
    const payload_ciphertext = try row.get([]u8, 5);
    const payload_tag_slice = try row.get([]u8, 6);

    const dek_nonce = try cp.toFixed(NONCE_LEN, dek_nonce_slice);
    const dek_tag = try cp.toFixed(TAG_LEN, dek_tag_slice);
    const payload_nonce = try cp.toFixed(NONCE_LEN, payload_nonce_slice);
    const payload_tag = try cp.toFixed(TAG_LEN, payload_tag_slice);
    const ciphertext_copy = try alloc.dupe(u8, payload_ciphertext);
    defer alloc.free(ciphertext_copy);
    const dek_copy = try alloc.dupe(u8, encrypted_dek);
    defer alloc.free(dek_copy);
    try result.drain();

    const kek = try cp.loadKekByVersion(alloc, kek_version);

    const dek_plain = try cp.decrypt(alloc, &dek_nonce, dek_copy, &dek_tag, &kek);
    defer alloc.free(dek_plain);

    const dek = try cp.toFixed(KEY_LEN, dek_plain);
    const plaintext_result = cp.decrypt(alloc, &payload_nonce, ciphertext_copy, &payload_tag, &dek) catch |err| {
        log.err("secret.decrypt_fail workspace_id={s} key_name={s} error_code=UZ-INTERNAL-003", .{ workspace_id, key_name });
        return err;
    };
    log.info("secret.retrieved workspace_id={s} key_name={s}", .{ workspace_id, key_name });
    return plaintext_result;
}

/// Re-encrypt a secret under a new KEK version. Safe to call during rotation:
/// reads the secret using the version stored in the DB row, writes it back
/// under new_kek_version. The ON CONFLICT upsert in store() atomically replaces
/// the old envelope with the new one.
pub fn reencryptSecret(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    new_kek_version: u32,
) !void {
    const plaintext = try load(alloc, conn, workspace_id, key_name);
    defer alloc.free(plaintext);
    try store(alloc, conn, workspace_id, key_name, plaintext, new_kek_version);
}

pub fn storeWorkspaceSkillSecret(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    skill_ref: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
    scope: cp.SkillSecretScope,
    secret_meta_json: []const u8,
    kek_version: u32,
) !void {
    const kek = try cp.loadKekByVersion(alloc, kek_version);

    var ws = try conn.query(
        "SELECT tenant_id FROM workspaces WHERE workspace_id = $1 LIMIT 1",
        .{workspace_id},
    );
    const ws_row = (try ws.next()) orelse {
        ws.deinit();
        return error.NotFound;
    };
    const tenant_id_raw = try ws_row.get([]const u8, 0);
    const tenant_id = try alloc.dupe(u8, tenant_id_raw);
    defer alloc.free(tenant_id);
    ws.drain() catch {};
    ws.deinit();

    var dek: [KEY_LEN]u8 = undefined;
    std.crypto.random.bytes(&dek);

    const wrapped_dek = try cp.encrypt(alloc, dek[0..], &kek);
    defer wrapped_dek.deinit(alloc);

    const encrypted_payload = try cp.encrypt(alloc, plaintext, &dek);
    defer encrypted_payload.deinit(alloc);

    const now_ms = std.time.milliTimestamp();

    const skill_secret_id = try id_format.generateSkillSecretId(alloc);
    defer alloc.free(skill_secret_id);
    _ = try conn.exec(
        \\INSERT INTO vault.workspace_skill_secrets
        \\  (id, tenant_id, workspace_id, skill_ref, key_name, scope, secret_meta_json, kek_version, encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, 1, $8, $9, $10, $11, $12, $13, $14, $14)
        \\ON CONFLICT (workspace_id, skill_ref, key_name) DO UPDATE
        \\SET tenant_id = EXCLUDED.tenant_id,
        \\    scope = EXCLUDED.scope,
        \\    secret_meta_json = EXCLUDED.secret_meta_json,
        \\    encrypted_dek = EXCLUDED.encrypted_dek,
        \\    dek_nonce = EXCLUDED.dek_nonce,
        \\    dek_tag = EXCLUDED.dek_tag,
        \\    nonce = EXCLUDED.nonce,
        \\    ciphertext = EXCLUDED.ciphertext,
        \\    tag = EXCLUDED.tag,
        \\    updated_at = EXCLUDED.updated_at
    , .{
        skill_secret_id,
        tenant_id,
        workspace_id,
        skill_ref,
        key_name,
        scope.label(),
        secret_meta_json,
        wrapped_dek.ciphertext,
        wrapped_dek.nonce[0..],
        wrapped_dek.tag[0..],
        encrypted_payload.nonce[0..],
        encrypted_payload.ciphertext,
        encrypted_payload.tag[0..],
        now_ms,
    });
}

pub fn deleteWorkspaceSkillSecret(
    conn: *pg.Conn,
    workspace_id: []const u8,
    skill_ref: []const u8,
    key_name: []const u8,
) !void {
    _ = try conn.exec(
        \\DELETE FROM vault.workspace_skill_secrets
        \\WHERE workspace_id = $1 AND skill_ref = $2 AND key_name = $3
    , .{ workspace_id, skill_ref, key_name });
}

test "reencryptSecret: plaintext recoverable after re-encryption with new KEK" {
    const alloc = std.testing.allocator;

    // Two distinct keys
    var raw_v1: [KEY_LEN]u8 = undefined;
    var raw_v2: [KEY_LEN]u8 = undefined;
    std.crypto.random.bytes(&raw_v1);
    std.crypto.random.bytes(&raw_v2);
    const hex_v1 = std.fmt.bytesToHex(raw_v1, .lower);
    const hex_v2 = std.fmt.bytesToHex(raw_v2, .lower);
    var hex_v1_z: [65]u8 = undefined;
    var hex_v2_z: [65]u8 = undefined;
    @memcpy(hex_v1_z[0..64], &hex_v1);
    hex_v1_z[64] = 0;
    @memcpy(hex_v2_z[0..64], &hex_v2);
    hex_v2_z[64] = 0;

    const c = @cImport(@cInclude("stdlib.h"));
    _ = c.setenv("ENCRYPTION_MASTER_KEY", &hex_v1_z, 1);
    _ = c.setenv("ENCRYPTION_MASTER_KEY_V2", &hex_v2_z, 1);
    defer _ = c.unsetenv("ENCRYPTION_MASTER_KEY");
    defer _ = c.unsetenv("ENCRYPTION_MASTER_KEY_V2");

    const plaintext = "rotation-test-secret-value";

    // Encrypt with v1 directly (low-level, no DB)
    const kek_v1 = try cp.loadKekByVersion(alloc, 1);
    const kek_v2 = try cp.loadKekByVersion(alloc, 2);

    const blob_v1 = try cp.encrypt(alloc, plaintext, &kek_v1);
    defer blob_v1.deinit(alloc);

    // Re-wrap DEK under v2: decrypt with v1, re-encrypt with v2
    const recovered_v1 = try cp.decrypt(alloc, &blob_v1.nonce, blob_v1.ciphertext, &blob_v1.tag, &kek_v1);
    defer alloc.free(recovered_v1);

    const blob_v2 = try cp.encrypt(alloc, recovered_v1, &kek_v2);
    defer blob_v2.deinit(alloc);

    const recovered_v2 = try cp.decrypt(alloc, &blob_v2.nonce, blob_v2.ciphertext, &blob_v2.tag, &kek_v2);
    defer alloc.free(recovered_v2);

    try std.testing.expectEqualStrings(plaintext, recovered_v2);

    // v1 key must not decrypt the v2 envelope
    try std.testing.expectError(
        cp.SecretError.DecryptFailed,
        cp.decrypt(alloc, &blob_v2.nonce, blob_v2.ciphertext, &blob_v2.tag, &kek_v1),
    );
}
