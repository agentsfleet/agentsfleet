//! AES-256-GCM primitives, types, and pure crypto helpers.
//! No pg or id_format dependencies — safe to import anywhere.

const std = @import("std");
const log = std.log.scoped(.secrets);

pub const AesGcm = std.crypto.aead.aes_gcm.Aes256Gcm;
pub const KEY_LEN = AesGcm.key_length; // 32
pub const NONCE_LEN = AesGcm.nonce_length; // 12
pub const TAG_LEN = AesGcm.tag_length; // 16

pub const SecretError = error{
    MissingMasterKey,
    InvalidKeyHex,
    InvalidEnvelope,
    DecryptFailed,
    NotFound,
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
        log.err("secret.master_key_not_set error_code=UZ-INTERNAL-003", .{});
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
            log.err("secret.unsupported_kek_version kek_version={d} error_code=UZ-INTERNAL-003", .{version});
            return SecretError.InvalidKeyHex;
        },
    };
    const hex = std.process.getEnvVarOwned(alloc, env_name) catch {
        log.err("secret.kek_env_not_set env={s} kek_version={d} error_code=UZ-INTERNAL-003", .{ env_name, version });
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

pub fn toFixed(comptime N: usize, bytes: []const u8) ![N]u8 {
    if (bytes.len != N) return SecretError.InvalidEnvelope;

    var out: [N]u8 = undefined;
    @memcpy(out[0..], bytes);
    return out;
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

