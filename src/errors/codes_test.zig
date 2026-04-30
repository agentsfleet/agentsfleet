// Tests for hint and credential error codes (migrated from codes.zig for M16_001).

const std = @import("std");
const ec = @import("error_registry.zig");

test "hint returns actionable text for known startup codes" {
    try std.testing.expect(ec.hint(ec.ERR_STARTUP_REDIS_CONNECT).len > 0);
    try std.testing.expect(ec.hint(ec.ERR_INTERNAL_DB_UNAVAILABLE).len > 0);
}

test "hint returns UNKNOWN hint for unregistered codes" {
    const h = ec.hint("UZ-NONEXISTENT-999");
    try std.testing.expectEqualStrings(ec.UNKNOWN.hint, h);
}

// ── T8 — OWASP Agent Security: credential error codes (M16_003) ──────

test "T8: ERR_CRED_* codes follow UZ-CRED- prefix naming (M16_003/M16_004)" {
    try std.testing.expect(std.mem.startsWith(u8, ec.ERR_CRED_ANTHROPIC_KEY_MISSING, "UZ-CRED-"));
    try std.testing.expect(std.mem.startsWith(u8, ec.ERR_CRED_GITHUB_TOKEN_FAILED, "UZ-CRED-"));
    try std.testing.expect(std.mem.startsWith(u8, ec.ERR_CRED_PLATFORM_KEY_MISSING, "UZ-CRED-"));
}

test "T8: ERR_CRED_* codes are distinct — no collision (M16_003/M16_004)" {
    try std.testing.expect(!std.mem.eql(u8, ec.ERR_CRED_ANTHROPIC_KEY_MISSING, ec.ERR_CRED_GITHUB_TOKEN_FAILED));
    try std.testing.expect(!std.mem.eql(u8, ec.ERR_CRED_ANTHROPIC_KEY_MISSING, ec.ERR_CRED_PLATFORM_KEY_MISSING));
    try std.testing.expect(!std.mem.eql(u8, ec.ERR_CRED_GITHUB_TOKEN_FAILED, ec.ERR_CRED_PLATFORM_KEY_MISSING));
}

test "T8: credential error hints are actionable and contain no raw secret values (M16_003)" {
    const anthropic_hint = ec.hint(ec.ERR_CRED_ANTHROPIC_KEY_MISSING);
    try std.testing.expect(anthropic_hint.len > 0);
    try std.testing.expect(std.mem.indexOf(u8, anthropic_hint, "sk-ant-api") == null);
    try std.testing.expect(std.mem.indexOf(u8, anthropic_hint, "Bearer ") == null);

    const github_hint = ec.hint(ec.ERR_CRED_GITHUB_TOKEN_FAILED);
    try std.testing.expect(github_hint.len > 0);
    try std.testing.expect(std.mem.indexOf(u8, github_hint, "ghs_") == null);
    try std.testing.expect(std.mem.indexOf(u8, github_hint, "ghp_") == null);
}

test "T8: ERR_CRED_GITHUB_TOKEN_FAILED hint references GitHub App (M16_003)" {
    const h = ec.hint(ec.ERR_CRED_GITHUB_TOKEN_FAILED);
    const mentions_app = std.mem.indexOf(u8, h, "App") != null or
        std.mem.indexOf(u8, h, "installation") != null or
        std.mem.indexOf(u8, h, "GITHUB_APP") != null;
    try std.testing.expect(mentions_app);
}

test "T8: ERR_CRED_PLATFORM_KEY_MISSING hint names admin endpoint and BYOK path (M16_004)" {
    const h = ec.hint(ec.ERR_CRED_PLATFORM_KEY_MISSING);
    try std.testing.expect(h.len > 0);
    const mentions_admin = std.mem.indexOf(u8, h, "platform-keys") != null or
        std.mem.indexOf(u8, h, "admin") != null;
    try std.testing.expect(mentions_admin);
    const mentions_byok = std.mem.indexOf(u8, h, "credentials/llm") != null or
        std.mem.indexOf(u8, h, "BYOK") != null or
        std.mem.indexOf(u8, h, "own key") != null;
    try std.testing.expect(mentions_byok);
    try std.testing.expect(std.mem.indexOf(u8, h, "sk-ant-") == null);
    try std.testing.expect(std.mem.indexOf(u8, h, "Bearer ") == null);
}
