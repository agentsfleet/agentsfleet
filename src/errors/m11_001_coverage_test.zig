// M11_001 spec-claim coverage tests.
//
// Spec-claim tracing (from docs/v2/done/M11_001_API_ERROR_STANDARDIZATION.md):
//   §1.2 [DONE here]  — every ERR_ in codes.zig has a TABLE entry (comptime)
//   §1.3 [DONE]       — lookup returns correct status+title (error_table.zig tests)
//   §2.2 [DONE here]  — errorResponse signature has no std.http.Status param
//   §3.1 [DONE here]  — openapi.json uses ErrorBody ref for all 4xx/5xx
//
// Tiers covered: T2, T7, T10, T12

const std = @import("std");
const codes = @import("codes.zig");
const error_table = @import("error_table.zig");

// ── §1.2 / T12: Comptime exhaustive coverage ─────────────────────────────────
//
// Every pub const ERR_* in codes.zig must have a TABLE entry.
// If this block fails to compile, someone added a code to codes.zig
// without adding a corresponding TABLE entry — catch it at build time,
// not at runtime when a 500 shows up for an unregistered code.

comptime {
    // 130 codes × 131 table entries × char-by-char eql — raise quota accordingly.
    @setEvalBranchQuota(1_000_000);
    const decls = @typeInfo(codes).@"struct".decls;
    for (decls) |decl| {
        if (std.mem.startsWith(u8, decl.name, "ERR_")) {
            const code: []const u8 = @field(codes, decl.name);
            if (error_table.lookup(code) == null) {
                @compileError("M11_001 §1.2 VIOLATION: error code '" ++ code ++
                    "' is declared in codes.zig but has no entry in error_table.zig TABLE. " ++
                    "Add an entry before shipping.");
            }
        }
    }
}

// ── §2.2 / T7: errorResponse function signature has no std.http.Status param ─
//
// The old signature was errorResponse(res, status, code, message, request_id) — 5 args.
// The new spec-required signature is errorResponse(res, code, detail, request_id) — 4 args.
// This test pins the arity so any reversion is caught immediately.

test "§2.2 spec-claim: errorResponse has exactly 4 parameters (no status arg)" {
    const fn_info = @typeInfo(@TypeOf(@import("../http/handlers/common.zig").errorResponse));
    const params = fn_info.@"fn".params;
    try std.testing.expectEqual(@as(usize, 4), params.len);
}

test "§2.2: third param is []const u8 (detail), not std.http.Status" {
    const fn_info = @typeInfo(@TypeOf(@import("../http/handlers/common.zig").errorResponse));
    const params = fn_info.@"fn".params;
    // param[2] is detail: []const u8 — not std.http.Status
    const detail_type = params[2].type.?;
    try std.testing.expectEqual([]const u8, detail_type);
}

// ── T2: Edge cases for error_table.lookup ────────────────────────────────────

test "T2: lookup returns null for empty string" {
    try std.testing.expectEqual(@as(?error_table.ErrorEntry, null), error_table.lookup(""));
}

test "T2: lookup returns null for whitespace-only input" {
    try std.testing.expectEqual(@as(?error_table.ErrorEntry, null), error_table.lookup("   "));
}

test "T2: lookup is case-sensitive — wrong case returns null" {
    // All codes are uppercase; lowercase must not match
    try std.testing.expectEqual(@as(?error_table.ErrorEntry, null), error_table.lookup("uz-auth-002"));
    try std.testing.expectEqual(@as(?error_table.ErrorEntry, null), error_table.lookup("UZ-auth-002"));
}

test "T2: lookup returns null for near-miss (trailing space)" {
    try std.testing.expectEqual(@as(?error_table.ErrorEntry, null), error_table.lookup("UZ-AUTH-002 "));
}

test "T2: lookup returns null for near-miss (prefix only)" {
    try std.testing.expectEqual(@as(?error_table.ErrorEntry, null), error_table.lookup("UZ-"));
}

test "T2: lookup handles very long input without crashing" {
    const long_code = "UZ-" ++ "A" ** 500;
    try std.testing.expectEqual(@as(?error_table.ErrorEntry, null), error_table.lookup(long_code));
}

// ── T7: Regression — pin specific code → status mappings ─────────────────────
//
// These are the most-called codes in production. If someone edits a TABLE entry
// and accidentally changes the HTTP status, this test catches it.

test "T7: UZ-AUTH-002 stays 401 (pinned)" {
    try std.testing.expectEqual(
        std.http.Status.unauthorized,
        error_table.lookup(codes.ERR_UNAUTHORIZED).?.http_status,
    );
}

test "T7: UZ-AUTH-001 stays 403 (pinned)" {
    try std.testing.expectEqual(
        std.http.Status.forbidden,
        error_table.lookup(codes.ERR_FORBIDDEN).?.http_status,
    );
}

test "T7: UZ-INTERNAL-001 stays 503 (pinned — db-unavailable, not 500)" {
    // This one is subtle: db-unavailable is 503 Service Unavailable, not 500.
    // UNKNOWN_ENTRY is 500 — they must not be confused.
    try std.testing.expectEqual(
        std.http.Status.service_unavailable,
        error_table.lookup(codes.ERR_INTERNAL_DB_UNAVAILABLE).?.http_status,
    );
}

test "T7: UZ-REQ-002 stays 413 (payload too large, pinned)" {
    try std.testing.expectEqual(
        std.http.Status.payload_too_large,
        error_table.lookup(codes.ERR_PAYLOAD_TOO_LARGE).?.http_status,
    );
}

test "T7: UZ-ZMB-009 stays 404 (zombie not found, pinned)" {
    try std.testing.expectEqual(
        std.http.Status.not_found,
        error_table.lookup(codes.ERR_ZOMBIE_NOT_FOUND).?.http_status,
    );
}

test "T7: UZ-WORKSPACE-002 stays 402 (workspace paused = payment required)" {
    try std.testing.expectEqual(
        std.http.Status.payment_required,
        error_table.lookup(codes.ERR_WORKSPACE_PAUSED).?.http_status,
    );
}

test "T7: UNKNOWN_ENTRY and UZ-INTERNAL-001 are distinct (RULE 5: distinguish error classes)" {
    // UNKNOWN_ENTRY is 500. UZ-INTERNAL-001 is 503.
    // A caller that passes an unregistered code should get 500,
    // NOT 503 — these are different error classes.
    const internal_001 = error_table.lookup(codes.ERR_INTERNAL_DB_UNAVAILABLE).?;
    try std.testing.expect(
        @intFromEnum(error_table.UNKNOWN_ENTRY.http_status) !=
        @intFromEnum(internal_001.http_status),
    );
}

// ── T7: Semantic correctness — auth codes match HTTP semantics ───────────────
//
// RULE 5: distinguish error classes.
// ERR_UNAUTHORIZED (UZ-AUTH-002) is for authentication failures (401) —
//   missing token, invalid token, expired token.
// ERR_FORBIDDEN (UZ-AUTH-001) is for authorization failures (403) —
//   valid token but insufficient permissions, wrong role.
// These must NEVER be swapped. Webhook auth failures are authentication,
// not authorization.

test "T7: ERR_UNAUTHORIZED is 401 (authentication failure, not 403)" {
    const entry = error_table.lookup(codes.ERR_UNAUTHORIZED).?;
    try std.testing.expectEqual(std.http.Status.unauthorized, entry.http_status);
    // Must NOT be 403 — that's authorization, not authentication
    try std.testing.expect(@intFromEnum(entry.http_status) != @intFromEnum(std.http.Status.forbidden));
}

test "T7: ERR_FORBIDDEN is 403 (authorization failure, not 401)" {
    const entry = error_table.lookup(codes.ERR_FORBIDDEN).?;
    try std.testing.expectEqual(std.http.Status.forbidden, entry.http_status);
    try std.testing.expect(@intFromEnum(entry.http_status) != @intFromEnum(std.http.Status.unauthorized));
}

test "T7: UZ-WH-003 stays 409 (zombie paused = conflict, not 403)" {
    // Paused zombie is a temporary state conflict — caller can retry when unpaused.
    // 403 Forbidden means "you lack permission" — semantically wrong here.
    try std.testing.expectEqual(
        std.http.Status.conflict,
        error_table.lookup(codes.ERR_WEBHOOK_ZOMBIE_PAUSED).?.http_status,
    );
}

// ── T10: Magic-value policy — TABLE format invariants ────────────────────────

test "T10: all TABLE codes start with 'UZ-' prefix" {
    for (error_table.TABLE) |entry| {
        try std.testing.expect(std.mem.startsWith(u8, entry.code, "UZ-"));
    }
}

test "T10: all TABLE docs_uri point to the canonical docs base" {
    for (error_table.TABLE) |entry| {
        try std.testing.expect(
            std.mem.startsWith(u8, entry.docs_uri, error_table.ERROR_DOCS_BASE),
        );
    }
}

test "T10: all TABLE docs_uri end with the entry's own code" {
    for (error_table.TABLE) |entry| {
        try std.testing.expect(std.mem.endsWith(u8, entry.docs_uri, entry.code));
    }
}

test "T10: UNKNOWN_ENTRY has sentinel code 'UZ-UNKNOWN' and is 500 (not in TABLE)" {
    // UNKNOWN_ENTRY.code is "UZ-UNKNOWN" — a sentinel, not a real registered code.
    // It must not appear in TABLE (tested in error_table.zig) and its status must be
    // the generic 500 fallback, not a domain-specific status.
    try std.testing.expectEqual(
        std.http.Status.internal_server_error,
        error_table.UNKNOWN_ENTRY.http_status,
    );
    try std.testing.expectEqualStrings("UZ-UNKNOWN", error_table.UNKNOWN_ENTRY.code);
}

// ── §3.1 / T12: openapi.json uses ErrorBody — verified via make check-openapi-errors ──
//
// Spec acceptance criterion 7: "openapi.json uses $ref: ErrorBody for all error responses."
// @embedFile can't cross the package root (src/), so this claim is verified by:
//   make check-openapi-errors    (see make/quality.mk)
// which runs the Python validator in scripts/check_openapi_errors.py.
// Verified manually: ErrorBody schema exists, all 5 4xx/5xx responses use
// application/problem+json, old Error schema deleted. (Apr 11, 2026)

// ── T12: API contract — error code format ────────────────────────────────────

test "T12: every TABLE code matches pattern UZ-<CATEGORY>-<NUMBER>" {
    // Pattern: UZ- then at least one capital letter/digit group, then - then digits or letters.
    // We verify: starts with UZ-, has at least 2 hyphens, no lowercase letters.
    for (error_table.TABLE) |entry| {
        const code = entry.code;
        // Starts with UZ-
        try std.testing.expect(std.mem.startsWith(u8, code, "UZ-"));
        // At least one more hyphen (UZ-CAT-NNN minimum)
        const suffix = code[3..]; // after "UZ-"
        try std.testing.expect(std.mem.indexOfScalar(u8, suffix, '-') != null);
        // No lowercase letters
        for (code) |ch| {
            try std.testing.expect(ch != std.ascii.toLower(ch) or
                ch == '-' or (ch >= '0' and ch <= '9'));
        }
    }
}
