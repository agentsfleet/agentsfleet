//! AES-256-GCM primitives, types, and pure crypto helpers.
//! No pg or id_format dependencies — safe to import anywhere.

const std = @import("std");
const common = @import("common");
const logging = @import("log");
const error_codes = @import("../errors/error_registry.zig");

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
    const Self = @This();

    nonce: [NONCE_LEN]u8,
    ciphertext: []u8,
    tag: [TAG_LEN]u8,

    pub fn deinit(self: Self, alloc: std.mem.Allocator) void {
        alloc.free(self.ciphertext);
    }
};

/// Process-wide Key-Encryption Key (KEK), resolved ONCE at boot from the
/// already-validated `ServeConfig.encryption_master_key` (single source of
/// truth) via `setKekFromHex`. Set before the server accepts traffic and
/// immutable after, so concurrent request threads read it without locking.
/// Replaces the prior per-call `ENCRYPTION_MASTER_KEY` env read.
var g_kek: ?[KEY_LEN]u8 = null;

/// Decode the 64-hex-char master key into the process KEK. Boot-only — call
/// once from `serve` startup with the config-resolved value.
pub fn setKekFromHex(hex: []const u8) SecretError!void {
    if (hex.len != KEY_LEN * 2) return SecretError.InvalidKeyHex;
    var key: [KEY_LEN]u8 = undefined;
    _ = std.fmt.hexToBytes(&key, hex) catch return SecretError.InvalidKeyHex;
    g_kek = key;
}

/// The boot-resolved 32-byte KEK. Errors if `setKekFromHex` has not run
/// (operator misconfig or a decrypt path reached before startup completed).
pub fn loadKek() SecretError![KEY_LEN]u8 {
    return g_kek orelse {
        log.err("kek_not_initialized", .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED });
        return SecretError.MissingMasterKey;
    };
}

/// Deterministic 32-byte test KEK (hex). Single source for every test that
/// reaches a vault store/load path — UFS one-value home for the key literal.
const TEST_KEK_HEX = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";

/// Option-C test convention: seed the process KEK the way `serve.run` does at
/// boot, replacing the retired `setenv("ENCRYPTION_MASTER_KEY", …)` env hack
/// (Zig 0.16's env snapshot made that mutation a no-op). The literal is valid
/// 64-hex, so `setKekFromHex` cannot fail here.
pub fn setTestKek() void {
    if (!@import("builtin").is_test) @compileError("setTestKek is test-only — never call it from the production build");
    setKekFromHex(TEST_KEK_HEX) catch |e| std.debug.panic("setTestKek: TEST_KEK_HEX must be valid 64-hex: {}", .{e});
}

/// Encrypt plaintext to raw binary components (nonce + ciphertext + tag).
pub fn encrypt(
    alloc: std.mem.Allocator,
    plaintext: []const u8,
    key: *const [KEY_LEN]u8,
) !EncryptedBlob {
    var nonce: [NONCE_LEN]u8 = undefined;
    try common.secureRandomBytes(&nonce);

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
    try common.secureRandomBytes(&key);

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
    try common.secureRandomBytes(&key);

    const blob = try encrypt(alloc, "hello", &key);
    defer blob.deinit(alloc);

    var bad_tag = blob.tag;
    bad_tag[0] ^= 0x01;

    try std.testing.expectError(
        SecretError.DecryptFailed,
        decrypt(alloc, &blob.nonce, blob.ciphertext, &bad_tag, &key),
    );
}

test "loadKek returns the KEK seeded via setKekFromHex (Option C boot-resolve)" {
    var key_bytes: [KEY_LEN]u8 = undefined;
    try common.secureRandomBytes(&key_bytes);

    const hex = std.fmt.bytesToHex(key_bytes, .lower);
    try setKekFromHex(&hex);

    const loaded = try loadKek();
    try std.testing.expectEqualSlices(u8, &key_bytes, &loaded);
}

test "setKekFromHex rejects a wrong-length hex (fails closed)" {
    try std.testing.expectError(SecretError.InvalidKeyHex, setKekFromHex("deadbeef"));
}
