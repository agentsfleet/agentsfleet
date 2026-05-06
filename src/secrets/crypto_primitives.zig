//! AES-256-GCM primitives, types, and pure crypto helpers.
//! No pg or id_format dependencies — safe to import anywhere.

const std = @import("std");
const logging = @import("log");
const log = logging.scoped(.secrets);

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
    UnsupportedKekVersion,
};

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const EncryptedBlob = struct {
    nonce: [NONCE_LEN]u8,
    ciphertext: []u8,
    tag: [TAG_LEN]u8,

    pub fn deinit(self: EncryptedBlob, alloc: std.mem.Allocator) void {
        alloc.free(self.ciphertext);
    }
};

/// Load the 32-byte KEK from `ENCRYPTION_MASTER_KEY` env var (64 hex chars).
pub fn loadKek(alloc: std.mem.Allocator) ![KEY_LEN]u8 {
    const hex = std.process.getEnvVarOwned(alloc, "ENCRYPTION_MASTER_KEY") catch {
        log.err("kek_env_not_set", .{ .error_code = "UZ-INTERNAL-003" });
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

test "loadKek reads ENCRYPTION_MASTER_KEY env var" {
    const alloc = std.testing.allocator;

    var key_bytes: [KEY_LEN]u8 = undefined;
    std.crypto.random.bytes(&key_bytes);

    const hex = std.fmt.bytesToHex(key_bytes, .lower);
    var hex_z: [65]u8 = undefined;
    @memcpy(hex_z[0..64], &hex);
    hex_z[64] = 0;

    const c = @cImport(@cInclude("stdlib.h"));
    _ = c.setenv("ENCRYPTION_MASTER_KEY", &hex_z, 1);
    defer _ = c.unsetenv("ENCRYPTION_MASTER_KEY");

    const loaded = try loadKek(alloc);
    try std.testing.expectEqualSlices(u8, &key_bytes, &loaded);
}

