//! Unit tests for api_keys.zig (non-DB paths).
//! DB-exercising dims 3.1-3.7 live in integration tests.

const std = @import("std");
const api_keys = @import("api_keys.zig");
const tenant_api_key = @import("../../auth/middleware/tenant_api_key.zig");

const testing = std.testing;

test "isValidKeyName accepts alphanumerics + hyphen + underscore" {
    try testing.expect(api_keys.isValidKeyName("ci-pipeline"));
    try testing.expect(api_keys.isValidKeyName("gh_actions_v2"));
    try testing.expect(api_keys.isValidKeyName("A1B2"));
    try testing.expect(api_keys.isValidKeyName("x"));
    const max_name = "a" ** api_keys.MAX_NAME_LEN;
    try testing.expect(api_keys.isValidKeyName(max_name));
}

test "isValidKeyName rejects empty, oversized, and illegal chars" {
    try testing.expect(!api_keys.isValidKeyName(""));
    const too_long = "a" ** (api_keys.MAX_NAME_LEN + 1);
    try testing.expect(!api_keys.isValidKeyName(too_long));
    try testing.expect(!api_keys.isValidKeyName("has space"));
    try testing.expect(!api_keys.isValidKeyName("has/slash"));
    try testing.expect(!api_keys.isValidKeyName("has.dot"));
}

test "generateRawKey produces zmb_t_ + 64 lower-hex chars" {
    const k = try api_keys.generateRawKey(testing.allocator);
    defer testing.allocator.free(k);
    try testing.expectEqual(@as(usize, 6 + 64), k.len);
    try testing.expect(std.mem.startsWith(u8, k, api_keys.KEY_PREFIX));
    for (k[6..]) |c| {
        const ok = (c >= '0' and c <= '9') or (c >= 'a' and c <= 'f');
        try testing.expect(ok);
    }
}

test "generateRawKey output is high-entropy across calls" {
    const k1 = try api_keys.generateRawKey(testing.allocator);
    defer testing.allocator.free(k1);
    const k2 = try api_keys.generateRawKey(testing.allocator);
    defer testing.allocator.free(k2);
    try testing.expect(!std.mem.eql(u8, k1, k2));
}

test "sortClauseFor recognizes exactly the allowed keys and rejects everything else" {
    try testing.expect(api_keys.sortClauseFor("created_at") != null);
    try testing.expect(api_keys.sortClauseFor("-created_at") != null);
    try testing.expect(api_keys.sortClauseFor("key_name") != null);
    try testing.expect(api_keys.sortClauseFor("-key_name") != null);
    try testing.expect(api_keys.sortClauseFor("id") == null);
    try testing.expect(api_keys.sortClauseFor("") == null);
    try testing.expect(api_keys.sortClauseFor("created_at; DROP TABLE") == null);
}

test "KEY_PREFIX is sourced from the middleware — no divergence between auth and issuance" {
    try testing.expectEqualStrings(tenant_api_key.TENANT_KEY_PREFIX, api_keys.KEY_PREFIX);
}
