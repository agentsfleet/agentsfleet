//! Database-backed secret storage using envelope encryption.
//! Depends on crypto_primitives for all crypto operations.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");
const cp = @import("crypto_primitives.zig");
const logging = @import("log");

const log = logging.scoped(.secrets);

const KEY_LEN = cp.KEY_LEN;
const NONCE_LEN = cp.NONCE_LEN;
const TAG_LEN = cp.TAG_LEN;

/// Store encrypted secret in vault.secrets with envelope encryption.
pub fn store(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
) !void {
    const kek = try cp.loadKek(alloc);

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
        \\  (id, workspace_id, key_name, encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
        \\ON CONFLICT (workspace_id, key_name) DO UPDATE
        \\SET encrypted_dek = EXCLUDED.encrypted_dek,
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
        wrapped_dek.ciphertext,
        wrapped_dek.nonce[0..],
        wrapped_dek.tag[0..],
        encrypted_payload.nonce[0..],
        encrypted_payload.ciphertext,
        encrypted_payload.tag[0..],
        now_ms,
    });
    log.info("stored", .{ .workspace_id = workspace_id, .key_name = key_name });
}

/// Load and decrypt a secret from vault.secrets.
pub fn load(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
) ![]u8 {
    var result = PgQuery.from(try conn.query(
        \\SELECT encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version
        \\FROM vault.secrets
        \\WHERE workspace_id = $1 AND key_name = $2
    , .{ workspace_id, key_name }));
    defer result.deinit();

    const row = try result.next() orelse {
        // Not-found is a normal control-flow path — caller decides whether to treat
        // it as an error. Log at debug so it doesn't trip "logged errors" test gates.
        log.debug("not_found", .{ .workspace_id = workspace_id, .key_name = key_name });
        return cp.SecretError.NotFound;
    };

    const encrypted_dek = try row.get([]u8, 0);
    const dek_nonce_slice = try row.get([]u8, 1);
    const dek_tag_slice = try row.get([]u8, 2);
    const payload_nonce_slice = try row.get([]u8, 3);
    const payload_ciphertext = try row.get([]u8, 4);
    const payload_tag_slice = try row.get([]u8, 5);
    // Only kek_version=1 is supported. Pre-cleanup, a multi-key dispatch path
    // (KEK_VERSION + ENCRYPTION_MASTER_KEY_V2) could store rows with version=2;
    // that path is gone, so any non-1 row would silently decrypt with the wrong
    // key and surface as DecryptFailed. Fail loud instead.
    const kek_version = try row.get(i32, 6);
    if (kek_version != 1) {
        log.err("unsupported_kek_version", .{
            .workspace_id = workspace_id,
            .key_name = key_name,
            .kek_version = kek_version,
            .error_code = "UZ-INTERNAL-003",
        });
        return cp.SecretError.UnsupportedKekVersion;
    }

    const dek_nonce = try cp.toFixed(NONCE_LEN, dek_nonce_slice);
    const dek_tag = try cp.toFixed(TAG_LEN, dek_tag_slice);
    const payload_nonce = try cp.toFixed(NONCE_LEN, payload_nonce_slice);
    const payload_tag = try cp.toFixed(TAG_LEN, payload_tag_slice);
    const ciphertext_copy = try alloc.dupe(u8, payload_ciphertext);
    defer alloc.free(ciphertext_copy);
    const dek_copy = try alloc.dupe(u8, encrypted_dek);
    defer alloc.free(dek_copy);

    const kek = try cp.loadKek(alloc);

    const dek_plain = try cp.decrypt(alloc, &dek_nonce, dek_copy, &dek_tag, &kek);
    defer alloc.free(dek_plain);

    const dek = try cp.toFixed(KEY_LEN, dek_plain);
    const plaintext_result = cp.decrypt(alloc, &payload_nonce, ciphertext_copy, &payload_tag, &dek) catch |err| {
        log.err("decrypt_failed", .{
            .workspace_id = workspace_id,
            .key_name = key_name,
            .error_code = "UZ-INTERNAL-003",
        });
        return err;
    };
    log.info("retrieved", .{ .workspace_id = workspace_id, .key_name = key_name });
    return plaintext_result;
}

