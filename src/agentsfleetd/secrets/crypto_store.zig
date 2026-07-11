//! Database-backed secret storage using envelope encryption.
//! Depends on crypto_primitives for all crypto operations.

const std = @import("std");
const constants = @import("common");
const clock = constants.clock;
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");
const cp = @import("crypto_primitives.zig");
const sql = @import("sql.zig");
const error_codes = @import("../errors/error_registry.zig");
const logging = @import("log");

const log = logging.scoped(.secrets);

const KEY_LEN = cp.KEY_LEN;
const NONCE_LEN = cp.NONCE_LEN;
const TAG_LEN = cp.TAG_LEN;
const KEK_VERSION_LEGACY: i32 = 1;
const KEK_VERSION_AAD_BOUND: i32 = 2;
const AAD_SEPARATOR: u8 = 0x1f;
const AAD_FORMAT = "{s}{c}{s}{c}{d}";

fn buildAad(alloc: std.mem.Allocator, workspace_id: []const u8, key_name: []const u8, kek_version: i32) ![]u8 {
    const canonical_workspace_id = try std.ascii.allocLowerString(alloc, workspace_id);
    defer alloc.free(canonical_workspace_id);
    return std.fmt.allocPrint(alloc, AAD_FORMAT, .{ canonical_workspace_id, AAD_SEPARATOR, key_name, AAD_SEPARATOR, kek_version });
}

/// Store encrypted secret in vault.secrets with envelope encryption.
pub fn store(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
) !void {
    var kek = try cp.loadKek();
    defer std.crypto.secureZero(u8, &kek);

    var dek: [KEY_LEN]u8 = undefined;
    defer std.crypto.secureZero(u8, &dek);
    try constants.secureRandomBytes(&dek);

    const aad = try buildAad(alloc, workspace_id, key_name, KEK_VERSION_AAD_BOUND);
    defer alloc.free(aad);

    const wrapped_dek = try cp.encrypt(alloc, dek[0..], aad, &kek);
    defer wrapped_dek.deinit(alloc);

    const encrypted_payload = try cp.encrypt(alloc, plaintext, aad, &dek);
    defer encrypted_payload.deinit(alloc);

    const now_ms = clock.nowMillis();

    const secret_id = try id_format.generateVaultSecretId(alloc);
    defer alloc.free(secret_id);
    _ = try conn.exec(sql.INSERT_SECRET, .{
        secret_id,
        workspace_id,
        key_name,
        wrapped_dek.ciphertext,
        wrapped_dek.nonce[0..],
        wrapped_dek.tag[0..],
        encrypted_payload.nonce[0..],
        encrypted_payload.ciphertext,
        encrypted_payload.tag[0..],
        KEK_VERSION_AAD_BOUND,
        now_ms,
    });
    // info (not debug) by design: credential store/retrieve stays visible in default prod logs for
    // security-access monitoring — key_name only, never the secret value. LOGGING_STANDARD §4 exception.
    log.info("stored", .{ .workspace_id = workspace_id, .key_name = key_name });
}

/// Load and decrypt a secret from vault.secrets.
pub fn load(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
) ![]u8 {
    var result = PgQuery.from(try conn.query(sql.SELECT_SECRET, .{ workspace_id, key_name }));
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
    const kek_version = try row.get(i32, 6);
    if (kek_version != KEK_VERSION_LEGACY and kek_version != KEK_VERSION_AAD_BOUND) {
        log.err("unsupported_kek_version", .{
            .workspace_id = workspace_id,
            .key_name = key_name,
            .kek_version = kek_version,
            .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED,
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

    var kek = try cp.loadKek();
    defer std.crypto.secureZero(u8, &kek);

    const aad = if (kek_version == KEK_VERSION_AAD_BOUND)
        try buildAad(alloc, workspace_id, key_name, kek_version)
    else
        try alloc.dupe(u8, "");
    defer alloc.free(aad);

    const dek_plain = try cp.decrypt(alloc, &dek_nonce, dek_copy, &dek_tag, aad, &kek);
    defer {
        std.crypto.secureZero(u8, dek_plain);
        alloc.free(dek_plain);
    }

    var dek = try cp.toFixed(KEY_LEN, dek_plain);
    defer std.crypto.secureZero(u8, &dek);
    const plaintext_result = cp.decrypt(alloc, &payload_nonce, ciphertext_copy, &payload_tag, aad, &dek) catch |err| {
        log.err("decrypt_failed", .{
            .workspace_id = workspace_id,
            .key_name = key_name,
            .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED,
        });
        return err;
    };
    // info (not debug) by design — security-access visibility, see store() above (§4 exception).
    log.info("retrieved", .{ .workspace_id = workspace_id, .key_name = key_name });
    return plaintext_result;
}
