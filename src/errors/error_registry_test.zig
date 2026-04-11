// M11_001 + M16_001 spec-claim coverage tests.
//
// M16_001: comptime validation (§1.2) moved into error_registry.zig itself.
// The ERR_* ↔ REGISTRY cross-check is now a comptime block in the registry,
// not a separate test — drift is structurally impossible.
//
// Remaining tests: errorResponse signature pins, lookup edge cases,
// regression pins, format invariants.
//
// Tiers covered: T2, T7, T10, T12

const std = @import("std");
const reg = @import("error_registry.zig");

// ── §2.2 / T7: errorResponse function signature has no std.http.Status param ─

test "§2.2 spec-claim: errorResponse has exactly 4 parameters (no status arg)" {
    const fn_info = @typeInfo(@TypeOf(@import("../http/handlers/common.zig").errorResponse));
    const params = fn_info.@"fn".params;
    try std.testing.expectEqual(@as(usize, 4), params.len);
}

test "§2.2: third param is []const u8 (detail), not std.http.Status" {
    const fn_info = @typeInfo(@TypeOf(@import("../http/handlers/common.zig").errorResponse));
    const params = fn_info.@"fn".params;
    const detail_type = params[2].type.?;
    try std.testing.expectEqual([]const u8, detail_type);
}

// ── T2: Edge cases for lookup ───────────────────────────────────────────────

test "T2: lookup returns UNKNOWN for empty string" {
    const entry = reg.lookup("");
    try std.testing.expectEqualStrings("UZ-UNKNOWN", entry.code);
}

test "T2: lookup returns UNKNOWN for whitespace-only input" {
    const entry = reg.lookup("   ");
    try std.testing.expectEqualStrings("UZ-UNKNOWN", entry.code);
}

test "T2: lookup is case-sensitive — wrong case returns UNKNOWN" {
    try std.testing.expectEqualStrings("UZ-UNKNOWN", reg.lookup("uz-auth-002").code);
    try std.testing.expectEqualStrings("UZ-UNKNOWN", reg.lookup("UZ-auth-002").code);
}

test "T2: lookup returns UNKNOWN for near-miss (trailing space)" {
    try std.testing.expectEqualStrings("UZ-UNKNOWN", reg.lookup("UZ-AUTH-002 ").code);
}

test "T2: lookup returns UNKNOWN for near-miss (prefix only)" {
    try std.testing.expectEqualStrings("UZ-UNKNOWN", reg.lookup("UZ-").code);
}

test "T2: lookup handles very long input without crashing" {
    const long_code = "UZ-" ++ "A" ** 500;
    try std.testing.expectEqualStrings("UZ-UNKNOWN", reg.lookup(long_code).code);
}

// ── T7: Regression — pin specific code → status mappings ────────────────────

test "T7: UZ-AUTH-002 stays 401 (pinned)" {
    try std.testing.expectEqual(
        std.http.Status.unauthorized,
        reg.lookup(reg.ERR_UNAUTHORIZED).http_status,
    );
}

test "T7: UZ-AUTH-001 stays 403 (pinned)" {
    try std.testing.expectEqual(
        std.http.Status.forbidden,
        reg.lookup(reg.ERR_FORBIDDEN).http_status,
    );
}

test "T7: UZ-INTERNAL-001 stays 503 (pinned — db-unavailable, not 500)" {
    try std.testing.expectEqual(
        std.http.Status.service_unavailable,
        reg.lookup(reg.ERR_INTERNAL_DB_UNAVAILABLE).http_status,
    );
}

test "T7: UZ-REQ-002 stays 413 (payload too large, pinned)" {
    try std.testing.expectEqual(
        std.http.Status.payload_too_large,
        reg.lookup(reg.ERR_PAYLOAD_TOO_LARGE).http_status,
    );
}

test "T7: UZ-ZMB-009 stays 404 (zombie not found, pinned)" {
    try std.testing.expectEqual(
        std.http.Status.not_found,
        reg.lookup(reg.ERR_ZOMBIE_NOT_FOUND).http_status,
    );
}

test "T7: UZ-WORKSPACE-002 stays 402 (workspace paused = payment required)" {
    try std.testing.expectEqual(
        std.http.Status.payment_required,
        reg.lookup(reg.ERR_WORKSPACE_PAUSED).http_status,
    );
}

test "T7: UNKNOWN and UZ-INTERNAL-001 are distinct (distinguish error classes)" {
    const internal_001 = reg.lookup(reg.ERR_INTERNAL_DB_UNAVAILABLE);
    try std.testing.expect(
        @intFromEnum(reg.UNKNOWN.http_status) != @intFromEnum(internal_001.http_status),
    );
}

test "T7: ERR_UNAUTHORIZED is 401 (authentication failure, not 403)" {
    const entry = reg.lookup(reg.ERR_UNAUTHORIZED);
    try std.testing.expectEqual(std.http.Status.unauthorized, entry.http_status);
    try std.testing.expect(@intFromEnum(entry.http_status) != @intFromEnum(std.http.Status.forbidden));
}

test "T7: ERR_FORBIDDEN is 403 (authorization failure, not 401)" {
    const entry = reg.lookup(reg.ERR_FORBIDDEN);
    try std.testing.expectEqual(std.http.Status.forbidden, entry.http_status);
    try std.testing.expect(@intFromEnum(entry.http_status) != @intFromEnum(std.http.Status.unauthorized));
}

test "T7: UZ-WH-003 stays 409 (zombie paused = conflict, not 403)" {
    try std.testing.expectEqual(
        std.http.Status.conflict,
        reg.lookup(reg.ERR_WEBHOOK_ZOMBIE_PAUSED).http_status,
    );
}

// ── T10: REGISTRY format invariants ─────────────────────────────────────────

test "T10: all REGISTRY codes start with 'UZ-' prefix" {
    for (reg.REGISTRY) |entry| {
        try std.testing.expect(std.mem.startsWith(u8, entry.code, "UZ-"));
    }
}

test "T10: all REGISTRY docs_uri point to the canonical docs base" {
    for (reg.REGISTRY) |entry| {
        try std.testing.expect(std.mem.startsWith(u8, entry.docs_uri, reg.ERROR_DOCS_BASE));
    }
}

test "T10: all REGISTRY docs_uri end with the entry's own code" {
    for (reg.REGISTRY) |entry| {
        try std.testing.expect(std.mem.endsWith(u8, entry.docs_uri, entry.code));
    }
}

test "T10: UNKNOWN has sentinel code 'UZ-UNKNOWN' and is 500" {
    try std.testing.expectEqual(std.http.Status.internal_server_error, reg.UNKNOWN.http_status);
    try std.testing.expectEqualStrings("UZ-UNKNOWN", reg.UNKNOWN.code);
}

// ── T10: all REGISTRY entries have non-empty hints ──────────────────────────

test "T10: every entry has a non-empty hint" {
    for (reg.REGISTRY) |entry| {
        try std.testing.expect(entry.hint.len > 0);
    }
}

// ── T12: API contract — error code format ───────────────────────────────────

test "T12: every REGISTRY code matches pattern UZ-<CATEGORY>-<NUMBER>" {
    for (reg.REGISTRY) |entry| {
        const code = entry.code;
        try std.testing.expect(std.mem.startsWith(u8, code, "UZ-"));
        const suffix = code[3..];
        try std.testing.expect(std.mem.indexOfScalar(u8, suffix, '-') != null);
        for (code) |ch| {
            try std.testing.expect(ch != std.ascii.toLower(ch) or
                ch == '-' or (ch >= '0' and ch <= '9'));
        }
    }
}

// ── M16_001: lookup() returns Entry, not ?Entry ─────────────────────────────

test "M16_001: lookup never returns null — unknown codes return UNKNOWN" {
    const entry = reg.lookup("UZ-DOES-NOT-EXIST");
    try std.testing.expectEqualStrings("UZ-UNKNOWN", entry.code);
    try std.testing.expectEqual(std.http.Status.internal_server_error, entry.http_status);
}

test "M16_001: lookup returns correct entry for known code" {
    const entry = reg.lookup("UZ-ZMB-009");
    try std.testing.expectEqual(std.http.Status.not_found, entry.http_status);
    try std.testing.expectEqualStrings("Zombie not found", entry.title);
    try std.testing.expect(entry.hint.len > 0);
    try std.testing.expect(std.mem.startsWith(u8, entry.docs_uri, reg.ERROR_DOCS_BASE));
}

test "M16_001: hint() returns non-empty string for all registered codes" {
    for (reg.REGISTRY) |entry| {
        const h = reg.hint(entry.code);
        try std.testing.expect(h.len > 0);
    }
}

test "M16_001: hint() returns UNKNOWN hint for unregistered codes" {
    const h = reg.hint("UZ-DOES-NOT-EXIST");
    try std.testing.expectEqualStrings(reg.UNKNOWN.hint, h);
}
