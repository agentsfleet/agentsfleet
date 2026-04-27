// Test-only signers that produce correctly signed webhook fixtures per source.
// Must NOT import from src/auth/middleware/* — signer and verifier share no code
// so a bug in one cannot silently cancel the other (RULE: signer/verifier isolation).

const std = @import("std");

const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;

pub const Source = enum { github, linear, slack, jira, svix };

pub const Signed = struct {
    header_name: []const u8,
    header_value: []const u8, // heap-owned
    pub fn deinit(self: Signed, alloc: std.mem.Allocator) void {
        alloc.free(self.header_value);
    }
};

const SignedSvix = struct {
    svix_id: []const u8, // heap-owned
    svix_timestamp: []const u8, // heap-owned
    svix_signature: []const u8, // heap-owned
    pub fn deinit(self: SignedSvix, alloc: std.mem.Allocator) void {
        alloc.free(self.svix_id);
        alloc.free(self.svix_timestamp);
        alloc.free(self.svix_signature);
    }
};

const SignedSlack = struct {
    signed: Signed,
    timestamp: []const u8, // heap-owned
    pub fn deinit(self: SignedSlack, alloc: std.mem.Allocator) void {
        self.signed.deinit(alloc);
        alloc.free(self.timestamp);
    }
};

fn hmacHex(alloc: std.mem.Allocator, key: []const u8, msg: []const u8, prefix: []const u8) ![]u8 {
    var mac: [HmacSha256.mac_length]u8 = undefined;
    HmacSha256.create(&mac, msg, key);
    const hex = std.fmt.bytesToHex(mac, .lower);
    const out = try alloc.alloc(u8, prefix.len + hex.len);
    @memcpy(out[0..prefix.len], prefix);
    @memcpy(out[prefix.len..], &hex);
    return out;
}

// GitHub: `x-hub-signature-256: sha256=<hex>` over raw body bytes.
pub fn signGithub(alloc: std.mem.Allocator, secret: []const u8, body: []const u8) !Signed {
    return .{
        .header_name = "x-hub-signature-256",
        .header_value = try hmacHex(alloc, secret, body, "sha256="),
    };
}

// Linear: `linear-signature: <hex>` over raw body bytes (no prefix).
fn signLinear(alloc: std.mem.Allocator, secret: []const u8, body: []const u8) !Signed {
    return .{
        .header_name = "linear-signature",
        .header_value = try hmacHex(alloc, secret, body, ""),
    };
}

// Jira: caller-chosen header; `sha256=<hex>` over raw body bytes.
fn signJira(alloc: std.mem.Allocator, secret: []const u8, header_name: []const u8, body: []const u8) !Signed {
    return .{
        .header_name = header_name,
        .header_value = try hmacHex(alloc, secret, body, "sha256="),
    };
}

// Slack v0: `x-slack-signature: v0=<hex>` over `v0:{ts}:{body}`. Timestamp seconds.
pub fn signSlack(alloc: std.mem.Allocator, secret: []const u8, ts_seconds: i64, body: []const u8) !SignedSlack {
    const ts_str = try std.fmt.allocPrint(alloc, "{d}", .{ts_seconds});
    errdefer alloc.free(ts_str);
    const basestring = try std.fmt.allocPrint(alloc, "v0:{s}:{s}", .{ ts_str, body });
    defer alloc.free(basestring);
    const header_value = try hmacHex(alloc, secret, basestring, "v0=");
    return .{
        .signed = .{ .header_name = "x-slack-signature", .header_value = header_value },
        .timestamp = ts_str,
    };
}

// Svix v1 single-signature: `v1,<base64>` over `{id}.{ts}.{body}`.
// `raw_key` is the decoded key bytes (middleware strips `whsec_` + base64-decodes).
fn signSvix(alloc: std.mem.Allocator, raw_key: []const u8, svix_id: []const u8, ts_seconds: i64, body: []const u8) !SignedSvix {
    const ts_str = try std.fmt.allocPrint(alloc, "{d}", .{ts_seconds});
    errdefer alloc.free(ts_str);
    const basestring = try std.fmt.allocPrint(alloc, "{s}.{s}.{s}", .{ svix_id, ts_str, body });
    defer alloc.free(basestring);

    var mac: [HmacSha256.mac_length]u8 = undefined;
    HmacSha256.create(&mac, basestring, raw_key);

    const enc = std.base64.standard.Encoder;
    const b64_len = enc.calcSize(mac.len);
    const b64_buf = try alloc.alloc(u8, b64_len);
    defer alloc.free(b64_buf);
    _ = enc.encode(b64_buf, &mac);

    const sig_header = try std.fmt.allocPrint(alloc, "v1,{s}", .{b64_buf});
    errdefer alloc.free(sig_header);
    const id_dup = try alloc.dupe(u8, svix_id);
    errdefer alloc.free(id_dup);

    return .{ .svix_id = id_dup, .svix_timestamp = ts_str, .svix_signature = sig_header };
}

// Build the `whsec_<base64>` form the Svix middleware expects in vault.
// Returned buffer is heap-owned.
fn svixKeyToVaultForm(alloc: std.mem.Allocator, raw_key: []const u8) ![]u8 {
    const enc = std.base64.standard.Encoder;
    const b64_len = enc.calcSize(raw_key.len);
    const total = "whsec_".len + b64_len;
    const out = try alloc.alloc(u8, total);
    errdefer alloc.free(out);
    @memcpy(out[0.."whsec_".len], "whsec_");
    _ = enc.encode(out["whsec_".len..], raw_key);
    return out;
}

// ─────────────────────────── Tests ───────────────────────────

test "signGithub produces 64-hex signature with sha256= prefix" {
    const alloc = std.testing.allocator;
    const s = try signGithub(alloc, "topsecret", "{\"event\":\"ping\"}");
    defer s.deinit(alloc);
    try std.testing.expectEqualStrings("x-hub-signature-256", s.header_name);
    try std.testing.expect(std.mem.startsWith(u8, s.header_value, "sha256="));
    try std.testing.expectEqual(@as(usize, "sha256=".len + 64), s.header_value.len);
}

test "signGithub deterministic — same inputs produce same output" {
    const alloc = std.testing.allocator;
    const a = try signGithub(alloc, "k", "body");
    defer a.deinit(alloc);
    const b = try signGithub(alloc, "k", "body");
    defer b.deinit(alloc);
    try std.testing.expectEqualStrings(a.header_value, b.header_value);
}

test "signGithub body mutation changes signature" {
    const alloc = std.testing.allocator;
    const a = try signGithub(alloc, "k", "body");
    defer a.deinit(alloc);
    const b = try signGithub(alloc, "k", "bodz");
    defer b.deinit(alloc);
    try std.testing.expect(!std.mem.eql(u8, a.header_value, b.header_value));
}

test "signLinear has no prefix and 64-hex body" {
    const alloc = std.testing.allocator;
    const s = try signLinear(alloc, "secret", "x");
    defer s.deinit(alloc);
    try std.testing.expectEqualStrings("linear-signature", s.header_name);
    try std.testing.expectEqual(@as(usize, 64), s.header_value.len);
}

test "signJira respects caller-chosen header" {
    const alloc = std.testing.allocator;
    const s = try signJira(alloc, "secret", "x-jira-hook-signature", "x");
    defer s.deinit(alloc);
    try std.testing.expectEqualStrings("x-jira-hook-signature", s.header_name);
    try std.testing.expect(std.mem.startsWith(u8, s.header_value, "sha256="));
}

test "signSlack basestring uses v0:{ts}:{body}" {
    const alloc = std.testing.allocator;
    const s = try signSlack(alloc, "secret", 1_700_000_000, "body");
    defer s.deinit(alloc);
    try std.testing.expectEqualStrings("x-slack-signature", s.signed.header_name);
    try std.testing.expect(std.mem.startsWith(u8, s.signed.header_value, "v0="));
    try std.testing.expectEqualStrings("1700000000", s.timestamp);
}

test "signSlack different timestamp changes signature" {
    const alloc = std.testing.allocator;
    const a = try signSlack(alloc, "k", 100, "b");
    defer a.deinit(alloc);
    const b = try signSlack(alloc, "k", 101, "b");
    defer b.deinit(alloc);
    try std.testing.expect(!std.mem.eql(u8, a.signed.header_value, b.signed.header_value));
}

test "signSvix produces v1,<b64> signature and returns id/ts" {
    const alloc = std.testing.allocator;
    const s = try signSvix(alloc, "raw-key-bytes-32xxxxxxxxxxxxxxxx", "msg_01", 1_700_000_000, "payload");
    defer s.deinit(alloc);
    try std.testing.expectEqualStrings("msg_01", s.svix_id);
    try std.testing.expectEqualStrings("1700000000", s.svix_timestamp);
    try std.testing.expect(std.mem.startsWith(u8, s.svix_signature, "v1,"));
}

test "signSvix id mutation changes signature" {
    const alloc = std.testing.allocator;
    const a = try signSvix(alloc, "key", "msg_A", 100, "body");
    defer a.deinit(alloc);
    const b = try signSvix(alloc, "key", "msg_B", 100, "body");
    defer b.deinit(alloc);
    try std.testing.expect(!std.mem.eql(u8, a.svix_signature, b.svix_signature));
}

test "svixKeyToVaultForm produces whsec_ prefix + valid base64" {
    const alloc = std.testing.allocator;
    const out = try svixKeyToVaultForm(alloc, "raw");
    defer alloc.free(out);
    try std.testing.expect(std.mem.startsWith(u8, out, "whsec_"));
    // base64 of "raw" (3 bytes) = 4 chars -> total length 6 + 4
    try std.testing.expectEqual(@as(usize, "whsec_".len + 4), out.len);
}
