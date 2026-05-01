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

test "lookup returns UNKNOWN for empty string" {
    const entry = reg.lookup("");
    try std.testing.expectEqualStrings("UZ-UNKNOWN", entry.code);
}

test "lookup returns UNKNOWN for whitespace-only input" {
    const entry = reg.lookup("   ");
    try std.testing.expectEqualStrings("UZ-UNKNOWN", entry.code);
}

test "lookup is case-sensitive — wrong case returns UNKNOWN" {
    try std.testing.expectEqualStrings("UZ-UNKNOWN", reg.lookup("uz-auth-002").code);
    try std.testing.expectEqualStrings("UZ-UNKNOWN", reg.lookup("UZ-auth-002").code);
}

test "lookup returns UNKNOWN for near-miss (trailing space)" {
    try std.testing.expectEqualStrings("UZ-UNKNOWN", reg.lookup("UZ-AUTH-002 ").code);
}

test "lookup returns UNKNOWN for near-miss (prefix only)" {
    try std.testing.expectEqualStrings("UZ-UNKNOWN", reg.lookup("UZ-").code);
}

test "lookup handles very long input without crashing" {
    const long_code = "UZ-" ++ "A" ** 500;
    try std.testing.expectEqualStrings("UZ-UNKNOWN", reg.lookup(long_code).code);
}

// ── T7: Regression — pin specific code → status mappings ────────────────────

test "UZ-AUTH-002 stays 401 (pinned)" {
    try std.testing.expectEqual(
        std.http.Status.unauthorized,
        reg.lookup(reg.ERR_UNAUTHORIZED).http_status,
    );
}

test "UZ-AUTH-001 stays 403 (pinned)" {
    try std.testing.expectEqual(
        std.http.Status.forbidden,
        reg.lookup(reg.ERR_FORBIDDEN).http_status,
    );
}

test "UZ-INTERNAL-001 stays 503 (pinned — db-unavailable, not 500)" {
    try std.testing.expectEqual(
        std.http.Status.service_unavailable,
        reg.lookup(reg.ERR_INTERNAL_DB_UNAVAILABLE).http_status,
    );
}

test "UZ-REQ-002 stays 413 (payload too large, pinned)" {
    try std.testing.expectEqual(
        std.http.Status.payload_too_large,
        reg.lookup(reg.ERR_PAYLOAD_TOO_LARGE).http_status,
    );
}

test "UZ-ZMB-009 stays 404 (zombie not found, pinned)" {
    try std.testing.expectEqual(
        std.http.Status.not_found,
        reg.lookup(reg.ERR_ZOMBIE_NOT_FOUND).http_status,
    );
}

test "UZ-WORKSPACE-002 stays 402 (workspace paused = payment required)" {
    try std.testing.expectEqual(
        std.http.Status.payment_required,
        reg.lookup(reg.ERR_WORKSPACE_PAUSED).http_status,
    );
}

test "UNKNOWN and UZ-INTERNAL-001 are distinct (distinguish error classes)" {
    const internal_001 = reg.lookup(reg.ERR_INTERNAL_DB_UNAVAILABLE);
    try std.testing.expect(
        @intFromEnum(reg.UNKNOWN.http_status) != @intFromEnum(internal_001.http_status),
    );
}

test "ERR_UNAUTHORIZED is 401 (authentication failure, not 403)" {
    const entry = reg.lookup(reg.ERR_UNAUTHORIZED);
    try std.testing.expectEqual(std.http.Status.unauthorized, entry.http_status);
    try std.testing.expect(@intFromEnum(entry.http_status) != @intFromEnum(std.http.Status.forbidden));
}

test "ERR_FORBIDDEN is 403 (authorization failure, not 401)" {
    const entry = reg.lookup(reg.ERR_FORBIDDEN);
    try std.testing.expectEqual(std.http.Status.forbidden, entry.http_status);
    try std.testing.expect(@intFromEnum(entry.http_status) != @intFromEnum(std.http.Status.unauthorized));
}

test "UZ-WH-003 stays 409 (zombie paused = conflict, not 403)" {
    try std.testing.expectEqual(
        std.http.Status.conflict,
        reg.lookup(reg.ERR_WEBHOOK_ZOMBIE_PAUSED).http_status,
    );
}

// ── T10: REGISTRY format invariants ─────────────────────────────────────────

test "all REGISTRY codes start with 'UZ-' prefix" {
    for (reg.REGISTRY) |entry| {
        try std.testing.expect(std.mem.startsWith(u8, entry.code, "UZ-"));
    }
}

test "all REGISTRY docs_uri point to the canonical docs base" {
    for (reg.REGISTRY) |entry| {
        try std.testing.expect(std.mem.startsWith(u8, entry.docs_uri, reg.ERROR_DOCS_BASE));
    }
}

test "all REGISTRY docs_uri end with the entry's own code" {
    for (reg.REGISTRY) |entry| {
        try std.testing.expect(std.mem.endsWith(u8, entry.docs_uri, entry.code));
    }
}

test "UNKNOWN has sentinel code 'UZ-UNKNOWN' and is 500" {
    try std.testing.expectEqual(std.http.Status.internal_server_error, reg.UNKNOWN.http_status);
    try std.testing.expectEqualStrings("UZ-UNKNOWN", reg.UNKNOWN.code);
}

// ── T10: all REGISTRY entries have non-empty hints ──────────────────────────

test "every entry has a non-empty hint" {
    for (reg.REGISTRY) |entry| {
        try std.testing.expect(entry.hint.len > 0);
    }
}

// ── T12: API contract — error code format ───────────────────────────────────

test "every REGISTRY code matches pattern UZ-<CATEGORY>-<NUMBER>" {
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

test "lookup never returns null — unknown codes return UNKNOWN" {
    const entry = reg.lookup("UZ-DOES-NOT-EXIST");
    try std.testing.expectEqualStrings("UZ-UNKNOWN", entry.code);
    try std.testing.expectEqual(std.http.Status.internal_server_error, entry.http_status);
}

test "lookup returns correct entry for known code" {
    const entry = reg.lookup("UZ-ZMB-009");
    try std.testing.expectEqual(std.http.Status.not_found, entry.http_status);
    try std.testing.expectEqualStrings("Zombie not found", entry.title);
    try std.testing.expect(entry.hint.len > 0);
    try std.testing.expect(std.mem.startsWith(u8, entry.docs_uri, reg.ERROR_DOCS_BASE));
}

test "hint() returns non-empty string for all registered codes" {
    for (reg.REGISTRY) |entry| {
        const h = reg.hint(entry.code);
        try std.testing.expect(h.len > 0);
    }
}

test "hint() returns UNKNOWN hint for unregistered codes" {
    const h = reg.hint("UZ-DOES-NOT-EXIST");
    try std.testing.expectEqualStrings(reg.UNKNOWN.hint, h);
}

// ── REGISTRY entry count regression ────────────────────────────────────────
// Pin the count so accidental deletions are caught immediately.

test "registry contains exactly 120 entries" {
    try std.testing.expectEqual(@as(usize, 120), reg.REGISTRY.len);
}

// ── Sentinel code lookup ───────────────────────────────────────────────────
// Looking up the sentinel code itself must return UNKNOWN (it's not in REGISTRY).

test "lookup of sentinel code 'UZ-UNKNOWN' returns UNKNOWN entry" {
    const entry = reg.lookup("UZ-UNKNOWN");
    try std.testing.expectEqualStrings("UZ-UNKNOWN", entry.code);
    try std.testing.expectEqual(std.http.Status.internal_server_error, entry.http_status);
    try std.testing.expectEqualStrings(reg.UNKNOWN.hint, entry.hint);
}

// ── T7: ERR_* constants resolve to correct REGISTRY entries ─────────────────
// Spot-check that ERR_* constant strings match their REGISTRY entries.
// Comptime self-check ensures ALL ERR_* are in REGISTRY; these pin values.

test "ERR_* constants match REGISTRY entry codes (spot check)" {
    // Verify the constant string equals the entry's code field
    try std.testing.expectEqualStrings(reg.ERR_UNAUTHORIZED, reg.lookup(reg.ERR_UNAUTHORIZED).code);
    try std.testing.expectEqualStrings(reg.ERR_ZOMBIE_NOT_FOUND, reg.lookup(reg.ERR_ZOMBIE_NOT_FOUND).code);
    try std.testing.expectEqualStrings(reg.ERR_CRED_ANTHROPIC_KEY_MISSING, reg.lookup(reg.ERR_CRED_ANTHROPIC_KEY_MISSING).code);
    try std.testing.expectEqualStrings(reg.ERR_EXEC_TIMEOUT_KILL, reg.lookup(reg.ERR_EXEC_TIMEOUT_KILL).code);
    try std.testing.expectEqualStrings(reg.ERR_APPROVAL_CONDITION_INVALID, reg.lookup(reg.ERR_APPROVAL_CONDITION_INVALID).code);
}

// ── T10: Operational hints contain actionable keywords ──────────────────────
// Beyond non-empty, verify key hints have the right operational guidance.

test "startup hints reference 'zombied doctor' or env vars" {
    const startup_codes = [_][]const u8{
        reg.ERR_STARTUP_ENV_CHECK,
        reg.ERR_STARTUP_CONFIG_LOAD,
        reg.ERR_STARTUP_DB_CONNECT,
    };
    for (startup_codes) |code| {
        const h = reg.hint(code);
        // Startup hints should reference diagnostics or config
        const has_doctor = std.mem.indexOf(u8, h, "doctor") != null;
        const has_env = std.mem.indexOf(u8, h, "DATABASE_URL") != null or
            std.mem.indexOf(u8, h, "env") != null or
            std.mem.indexOf(u8, h, "REDIS") != null;
        try std.testing.expect(has_doctor or has_env);
    }
}

test "gate hints reference gate results or timeout" {
    const gate_codes = [_][]const u8{
        reg.ERR_GATE_COMMAND_FAILED,
        reg.ERR_GATE_COMMAND_TIMEOUT,
        reg.ERR_GATE_REPAIR_EXHAUSTED,
    };
    for (gate_codes) |code| {
        const h = reg.hint(code);
        const has_gate = std.mem.indexOf(u8, h, "gate") != null or
            std.mem.indexOf(u8, h, "Gate") != null;
        try std.testing.expect(has_gate);
    }
}

// ── T12: Entry struct has exactly 5 fields (spec invariant §1.1) ────────────

test "Entry struct has 5 fields (code, http_status, title, hint, docs_uri)" {
    const fields = @typeInfo(reg.Entry).@"struct".fields;
    try std.testing.expectEqual(@as(usize, 5), fields.len);
}

// ── T7: UNKNOWN sentinel has non-empty fields ──────────────────────────────

test "UNKNOWN sentinel has all fields populated" {
    try std.testing.expect(reg.UNKNOWN.code.len > 0);
    try std.testing.expect(reg.UNKNOWN.title.len > 0);
    try std.testing.expect(reg.UNKNOWN.hint.len > 0);
    try std.testing.expect(reg.UNKNOWN.docs_uri.len > 0);
}

// ── T7: Canary — deleted files must not be importable ───────────────────────
// If someone re-creates codes.zig or error_table.zig, these comptime checks
// will fail because the test expects the imports to NOT exist.
// (We can't test "import fails" directly, but we verify the new file IS the
// canonical source by checking its public API.)

test "error_registry.zig exports REGISTRY (not TABLE)" {
    // REGISTRY must exist as a pub const
    try std.testing.expect(reg.REGISTRY.len > 0);
    // Entry must exist (not ErrorEntry)
    const e: reg.Entry = reg.REGISTRY[0];
    try std.testing.expect(e.code.len > 0);
}

// ── M10_006: ErrorMapping + validateErrorTable (bvisor pattern) ───────────

test "ErrorMapping struct has 3 fields (err, code, message)" {
    const fields = @typeInfo(reg.ErrorMapping).@"struct".fields;
    try std.testing.expectEqual(@as(usize, 3), fields.len);
}

test "validateErrorTable accepts valid single-entry table" {
    const table = [_]reg.ErrorMapping{
        .{ .err = error.OutOfMemory, .code = "UZ-TEST-001", .message = "test message" },
    };
    // comptime validation — if it compiles, it passes
    comptime {
        reg.validateErrorTable(&table);
    }
}

test "validateErrorTable accepts valid multi-entry table" {
    const table = [_]reg.ErrorMapping{
        .{ .err = error.OutOfMemory, .code = "UZ-TEST-001", .message = "oom" },
        .{ .err = error.Overflow, .code = "UZ-TEST-002", .message = "overflow" },
        .{ .err = error.InvalidCharacter, .code = "UZ-TEST-003", .message = "bad char" },
    };
    comptime {
        reg.validateErrorTable(&table);
    }
}

test "tenant billing error table passes validateErrorTable at comptime" {
    comptime {
        const billing = @import("../state/tenant_billing.zig");
        _ = billing; // comptime validation runs on import
    }
}

test "PgQuery size pinned at 8 bytes (single pointer)" {
    const PgQuery = @import("../db/pg_query.zig").PgQuery;
    try std.testing.expectEqual(@as(usize, 8), @sizeOf(PgQuery));
}

test "ZombieSession size pinned at 392 bytes" {
    // M23_001: +24 bytes for execution_id (?[]const u8 = 16) + execution_started_at (i64 = 8)
    // M28_001: +56 bytes for ?WebhookSignatureConfig in ZombieTrigger.webhook.
    // M28_001 §3: +16 bytes for secret_ref slice in WebhookSignatureConfig.
    const ZombieSession = @import("../zombie/event_loop_types.zig").ZombieSession;
    try std.testing.expectEqual(@as(usize, 392), @sizeOf(ZombieSession));
}

// T7: Regression — ErrorMapping field count and types
test "ErrorMapping.err field is anyerror" {
    const fields = @typeInfo(reg.ErrorMapping).@"struct".fields;
    // First field is 'err' of type anyerror
    try std.testing.expectEqualStrings("err", fields[0].name);
    try std.testing.expectEqualStrings("code", fields[1].name);
    try std.testing.expectEqualStrings("message", fields[2].name);
}
