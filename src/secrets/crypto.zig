//! AES-256-GCM secret storage helpers.
//! KEK: 32-byte hex from ENCRYPTION_MASTER_KEY env var.
//! Storage format: BYTEA columns (nonce, ciphertext, tag).

const std = @import("std");
const pg = @import("pg");
const log = std.log.scoped(.secrets);
const id_format = @import("../types/id_format.zig");

const AesGcm = std.crypto.aead.aes_gcm.Aes256Gcm;
const KEY_LEN = AesGcm.key_length; // 32
const NONCE_LEN = AesGcm.nonce_length; // 12
const TAG_LEN = AesGcm.tag_length; // 16

pub const SecretError = error{
    MissingMasterKey,
    InvalidKeyHex,
    InvalidEnvelope,
    DecryptFailed,
    NotFound,
};

pub const SkillSecretScope = enum {
    host,
    sandbox,

    pub fn label(self: SkillSecretScope) []const u8 {
        return switch (self) {
            .host => "host",
            .sandbox => "sandbox",
        };
    }
};

pub const EnvPair = struct {
    name: []u8,
    value: []u8,
};

pub const SecretInjectionPlan = struct {
    host_env: []EnvPair,
    sandbox_env: []EnvPair,

    pub fn deinit(self: SecretInjectionPlan, alloc: std.mem.Allocator) void {
        for (self.host_env) |entry| {
            alloc.free(entry.name);
            alloc.free(entry.value);
        }
        alloc.free(self.host_env);
        for (self.sandbox_env) |entry| {
            alloc.free(entry.name);
            alloc.free(entry.value);
        }
        alloc.free(self.sandbox_env);
    }
};

pub const EncryptedBlob = struct {
    nonce: [NONCE_LEN]u8,
    ciphertext: []u8,
    tag: [TAG_LEN]u8,

    pub fn deinit(self: EncryptedBlob, alloc: std.mem.Allocator) void {
        alloc.free(self.ciphertext);
    }
};

/// Load the 32-byte master key from ENCRYPTION_MASTER_KEY env var (64 hex chars).
pub fn loadKek(alloc: std.mem.Allocator) ![KEY_LEN]u8 {
    const hex = std.process.getEnvVarOwned(alloc, "ENCRYPTION_MASTER_KEY") catch {
        log.err("ENCRYPTION_MASTER_KEY not set", .{});
        return SecretError.MissingMasterKey;
    };
    defer alloc.free(hex);

    if (hex.len != KEY_LEN * 2) return SecretError.InvalidKeyHex;

    var key: [KEY_LEN]u8 = undefined;
    _ = try std.fmt.hexToBytes(&key, hex);
    return key;
}

/// Backward-compatible alias.
pub fn loadMasterKey(alloc: std.mem.Allocator) ![KEY_LEN]u8 {
    return loadKek(alloc);
}

/// Load KEK by version number. Version 1 reads ENCRYPTION_MASTER_KEY,
/// version 2 reads ENCRYPTION_MASTER_KEY_V2.
pub fn loadKekByVersion(alloc: std.mem.Allocator, version: u32) ![KEY_LEN]u8 {
    const env_name = switch (version) {
        1 => "ENCRYPTION_MASTER_KEY",
        2 => "ENCRYPTION_MASTER_KEY_V2",
        else => {
            log.err("unsupported kek_version={d}", .{version});
            return SecretError.InvalidKeyHex;
        },
    };
    const hex = std.process.getEnvVarOwned(alloc, env_name) catch {
        log.err("{s} not set (kek_version={d})", .{ env_name, version });
        return SecretError.MissingMasterKey;
    };
    defer alloc.free(hex);

    if (hex.len != KEY_LEN * 2) return SecretError.InvalidKeyHex;

    var key: [KEY_LEN]u8 = undefined;
    _ = try std.fmt.hexToBytes(&key, hex);
    return key;
}

/// Encrypt plaintext to raw binary components (nonce + ciphertext + tag).
pub fn encrypt(
    alloc: std.mem.Allocator,
    plaintext: []const u8,
    key: *const [KEY_LEN]u8,
) !EncryptedBlob {
    var nonce: [NONCE_LEN]u8 = undefined;
    std.crypto.random.bytes(&nonce);

    const ciphertext = try alloc.alloc(u8, plaintext.len);
    errdefer alloc.free(ciphertext);

    var tag: [TAG_LEN]u8 = undefined;
    AesGcm.encrypt(ciphertext, &tag, plaintext, "", nonce, key.*);

    return .{
        .nonce = nonce,
        .ciphertext = ciphertext,
        .tag = tag,
    };
}

/// Decrypt raw binary components into plaintext.
/// Caller owns returned slice.
pub fn decrypt(
    alloc: std.mem.Allocator,
    nonce: *const [NONCE_LEN]u8,
    ciphertext: []const u8,
    tag: *const [TAG_LEN]u8,
    key: *const [KEY_LEN]u8,
) ![]u8 {
    const plaintext = try alloc.alloc(u8, ciphertext.len);
    errdefer alloc.free(plaintext);

    AesGcm.decrypt(plaintext, ciphertext, tag.*, "", nonce.*, key.*) catch
        return SecretError.DecryptFailed;

    return plaintext;
}

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
    const kek = try loadKekByVersion(alloc, kek_version);

    var dek: [KEY_LEN]u8 = undefined;
    std.crypto.random.bytes(&dek);

    const wrapped_dek = try encrypt(alloc, dek[0..], &kek);
    defer wrapped_dek.deinit(alloc);

    const encrypted_payload = try encrypt(alloc, plaintext, &dek);
    defer encrypted_payload.deinit(alloc);

    const now_ms = std.time.milliTimestamp();

    const secret_id = try id_format.generateVaultSecretId(alloc);
    defer alloc.free(secret_id);
    var result = try conn.query(
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
    result.deinit();
}

fn toFixed(comptime N: usize, bytes: []const u8) ![N]u8 {
    if (bytes.len != N) return SecretError.InvalidEnvelope;

    var out: [N]u8 = undefined;
    @memcpy(out[0..], bytes);
    return out;
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

    const row = try result.next() orelse return SecretError.NotFound;

    const kek_version = @as(u32, @intCast(try row.get(i32, 0)));
    const encrypted_dek = try row.get([]u8, 1);
    const dek_nonce_slice = try row.get([]u8, 2);
    const dek_tag_slice = try row.get([]u8, 3);
    const payload_nonce_slice = try row.get([]u8, 4);
    const payload_ciphertext = try row.get([]u8, 5);
    const payload_tag_slice = try row.get([]u8, 6);

    const dek_nonce = try toFixed(NONCE_LEN, dek_nonce_slice);
    const dek_tag = try toFixed(TAG_LEN, dek_tag_slice);
    const payload_nonce = try toFixed(NONCE_LEN, payload_nonce_slice);
    const payload_tag = try toFixed(TAG_LEN, payload_tag_slice);

    const kek = try loadKekByVersion(alloc, kek_version);

    const dek_plain = try decrypt(alloc, &dek_nonce, encrypted_dek, &dek_tag, &kek);
    defer alloc.free(dek_plain);

    const dek = try toFixed(KEY_LEN, dek_plain);
    return decrypt(alloc, &payload_nonce, payload_ciphertext, &payload_tag, &dek);
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
    scope: SkillSecretScope,
    secret_meta_json: []const u8,
    kek_version: u32,
) !void {
    const kek = try loadKekByVersion(alloc, kek_version);

    var ws = try conn.query(
        "SELECT tenant_id FROM workspaces WHERE workspace_id = $1 LIMIT 1",
        .{workspace_id},
    );
    defer ws.deinit();
    const ws_row = (try ws.next()) orelse return error.NotFound;
    const tenant_id = try ws_row.get([]const u8, 0);

    var dek: [KEY_LEN]u8 = undefined;
    std.crypto.random.bytes(&dek);

    const wrapped_dek = try encrypt(alloc, dek[0..], &kek);
    defer wrapped_dek.deinit(alloc);

    const encrypted_payload = try encrypt(alloc, plaintext, &dek);
    defer encrypted_payload.deinit(alloc);

    const now_ms = std.time.milliTimestamp();

    const skill_secret_id = try id_format.generateSkillSecretId(alloc);
    defer alloc.free(skill_secret_id);
    var result = try conn.query(
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
    result.deinit();
}

pub fn deleteWorkspaceSkillSecret(
    conn: *pg.Conn,
    workspace_id: []const u8,
    skill_ref: []const u8,
    key_name: []const u8,
) !void {
    var result = try conn.query(
        \\DELETE FROM vault.workspace_skill_secrets
        \\WHERE workspace_id = $1 AND skill_ref = $2 AND key_name = $3
    , .{ workspace_id, skill_ref, key_name });
    result.deinit();
}

pub fn buildSecretInjectionPlan(
    alloc: std.mem.Allocator,
    keys: []const []const u8,
    values: []const []const u8,
    scopes: []const SkillSecretScope,
) !SecretInjectionPlan {
    if (keys.len != values.len or keys.len != scopes.len) return SecretError.InvalidEnvelope;

    var host_env: std.ArrayList(EnvPair) = .{};
    errdefer {
        for (host_env.items) |entry| {
            alloc.free(entry.name);
            alloc.free(entry.value);
        }
        host_env.deinit(alloc);
    }

    var sandbox_env: std.ArrayList(EnvPair) = .{};
    errdefer {
        for (sandbox_env.items) |entry| {
            alloc.free(entry.name);
            alloc.free(entry.value);
        }
        sandbox_env.deinit(alloc);
    }

    for (keys, values, scopes) |key, value, scope| {
        const env_name = try normalizeSkillSecretEnvName(alloc, key, scope);
        const env_value = try alloc.dupe(u8, value);
        const entry: EnvPair = .{ .name = env_name, .value = env_value };
        switch (scope) {
            .host => try host_env.append(alloc, entry),
            .sandbox => try sandbox_env.append(alloc, entry),
        }
    }

    return .{
        .host_env = try host_env.toOwnedSlice(alloc),
        .sandbox_env = try sandbox_env.toOwnedSlice(alloc),
    };
}

fn normalizeSkillSecretEnvName(
    alloc: std.mem.Allocator,
    key_name: []const u8,
    scope: SkillSecretScope,
) ![]u8 {
    const prefix = switch (scope) {
        .host => "UZ_HOST_SKILL_",
        .sandbox => "UZ_SANDBOX_SKILL_",
    };
    var out: std.ArrayList(u8) = .{};
    defer out.deinit(alloc);
    try out.appendSlice(alloc, prefix);
    for (key_name) |c| {
        if (std.ascii.isAlphanumeric(c)) {
            try out.append(alloc, std.ascii.toUpper(c));
        } else {
            try out.append(alloc, '_');
        }
    }
    return out.toOwnedSlice(alloc);
}

test "encrypt/decrypt round-trip with raw bytes" {
    const alloc = std.testing.allocator;

    var key: [KEY_LEN]u8 = undefined;
    std.crypto.random.bytes(&key);

    const plaintext = "super-secret-api-key-12345";
    const blob = try encrypt(alloc, plaintext, &key);
    defer blob.deinit(alloc);

    const recovered = try decrypt(alloc, &blob.nonce, blob.ciphertext, &blob.tag, &key);
    defer alloc.free(recovered);

    try std.testing.expectEqualStrings(plaintext, recovered);
}

test "decrypt fails when tag is tampered" {
    const alloc = std.testing.allocator;

    var key: [KEY_LEN]u8 = undefined;
    std.crypto.random.bytes(&key);

    const blob = try encrypt(alloc, "hello", &key);
    defer blob.deinit(alloc);

    var bad_tag = blob.tag;
    bad_tag[0] ^= 0x01;

    try std.testing.expectError(
        SecretError.DecryptFailed,
        decrypt(alloc, &blob.nonce, blob.ciphertext, &bad_tag, &key),
    );
}

test "loadKekByVersion dispatches to correct env var" {
    const alloc = std.testing.allocator;

    var key_v1: [KEY_LEN]u8 = undefined;
    std.crypto.random.bytes(&key_v1);
    var key_v2: [KEY_LEN]u8 = undefined;
    std.crypto.random.bytes(&key_v2);

    const hex_v1 = std.fmt.bytesToHex(key_v1, .lower);
    const hex_v2 = std.fmt.bytesToHex(key_v2, .lower);
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

    const loaded_v1 = try loadKekByVersion(alloc, 1);
    const loaded_v2 = try loadKekByVersion(alloc, 2);

    try std.testing.expectEqualSlices(u8, &key_v1, &loaded_v1);
    try std.testing.expectEqualSlices(u8, &key_v2, &loaded_v2);
    try std.testing.expect(!std.mem.eql(u8, &loaded_v1, &loaded_v2));
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
    const kek_v1 = try loadKekByVersion(alloc, 1);
    const kek_v2 = try loadKekByVersion(alloc, 2);

    const blob_v1 = try encrypt(alloc, plaintext, &kek_v1);
    defer blob_v1.deinit(alloc);

    // Re-wrap DEK under v2: decrypt with v1, re-encrypt with v2
    const recovered_v1 = try decrypt(alloc, &blob_v1.nonce, blob_v1.ciphertext, &blob_v1.tag, &kek_v1);
    defer alloc.free(recovered_v1);

    const blob_v2 = try encrypt(alloc, recovered_v1, &kek_v2);
    defer blob_v2.deinit(alloc);

    const recovered_v2 = try decrypt(alloc, &blob_v2.nonce, blob_v2.ciphertext, &blob_v2.tag, &kek_v2);
    defer alloc.free(recovered_v2);

    try std.testing.expectEqualStrings(plaintext, recovered_v2);

    // v1 key must not decrypt the v2 envelope
    try std.testing.expectError(
        SecretError.DecryptFailed,
        decrypt(alloc, &blob_v2.nonce, blob_v2.ciphertext, &blob_v2.tag, &kek_v1),
    );
}

test "buildSecretInjectionPlan keeps host and sandbox scopes separate" {
    const keys = [_][]const u8{ "api_key", "session-token" };
    const values = [_][]const u8{ "k1", "k2" };
    const scopes = [_]SkillSecretScope{ .host, .sandbox };
    const plan = try buildSecretInjectionPlan(std.testing.allocator, &keys, &values, &scopes);
    defer plan.deinit(std.testing.allocator);

    try std.testing.expectEqual(@as(usize, 1), plan.host_env.len);
    try std.testing.expectEqual(@as(usize, 1), plan.sandbox_env.len);
    try std.testing.expectEqualStrings("UZ_HOST_SKILL_API_KEY", plan.host_env[0].name);
    try std.testing.expectEqualStrings("UZ_SANDBOX_SKILL_SESSION_TOKEN", plan.sandbox_env[0].name);
}
