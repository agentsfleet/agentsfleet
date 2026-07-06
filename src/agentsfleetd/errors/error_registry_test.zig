// Coverage for the error registry: errorResponse signature pins, lookup edge
// cases, code↔status regression pins, REGISTRY format invariants. The
// ERR_* ↔ REGISTRY cross-check is a comptime block in error_registry.zig
// itself — drift is structurally impossible.

const std = @import("std");
const reg = @import("error_registry.zig");

// ── errorResponse signature pins (no std.http.Status param) ────────────────

test "errorResponse has exactly 4 parameters (no status arg)" {
    const fn_info = @typeInfo(@TypeOf(@import("../http/handlers/common.zig").errorResponse));
    const params = fn_info.@"fn".params;
    try std.testing.expectEqual(@as(usize, 4), params.len);
}

test "errorResponse third param is []const u8 (detail), not std.http.Status" {
    const fn_info = @typeInfo(@TypeOf(@import("../http/handlers/common.zig").errorResponse));
    const params = fn_info.@"fn".params;
    const detail_type = params[2].type.?;
    try std.testing.expectEqual([]const u8, detail_type);
}

// ── Edge cases for lookup ──────────────────────────────────────────────────

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

// ── Regression: pin specific code → status mappings ───────────────────────

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

test "UZ-AGT-009 stays 404 (fleet not found, pinned)" {
    try std.testing.expectEqual(
        std.http.Status.not_found,
        reg.lookup(reg.ERR_AGENTSFLEET_NOT_FOUND).http_status,
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

test "UZ-AGT-012 is 409 (paused steer = conflict, with a resume hint)" {
    const entry = reg.lookup(reg.ERR_AGENTSFLEET_PAUSED_INGRESS);
    try std.testing.expectEqual(std.http.Status.conflict, entry.http_status);
    try std.testing.expect(std.mem.indexOf(u8, entry.hint, "agentsfleet resume") != null);
}

test "UZ-AUTH-014 is 409 (pending session is still approvable — not 410 Gone)" {
    // session_verify_consume.lua returns not_approved WITHOUT consuming the
    // session; the caller can still approve it in the dashboard and retry.
    const entry = reg.lookup(reg.ERR_SESSION_NOT_APPROVED);
    try std.testing.expectEqual(std.http.Status.conflict, entry.http_status);
    try std.testing.expect(@intFromEnum(entry.http_status) != @intFromEnum(std.http.Status.gone));
}

// ── REGISTRY format invariants ─────────────────────────────────────────────

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

// ── All REGISTRY entries have non-empty hints ─────────────────────────────

test "every entry has a non-empty hint" {
    for (reg.REGISTRY) |entry| {
        try std.testing.expect(entry.hint.len > 0);
    }
}

// ── API contract: error code format ───────────────────────────────────────

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

// ── lookup() returns Entry, not ?Entry ────────────────────────────────────

test "lookup never returns null — unknown codes return UNKNOWN" {
    const entry = reg.lookup("UZ-DOES-NOT-EXIST");
    try std.testing.expectEqualStrings("UZ-UNKNOWN", entry.code);
    try std.testing.expectEqual(std.http.Status.internal_server_error, entry.http_status);
}

test "lookup returns correct entry for known code" {
    const entry = reg.lookup("UZ-AGT-009");
    try std.testing.expectEqual(std.http.Status.not_found, entry.http_status);
    try std.testing.expectEqualStrings("Fleet not found", entry.title);
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

// ── Sentinel code lookup ───────────────────────────────────────────────────
// Looking up the sentinel code itself must return UNKNOWN (it's not in REGISTRY).

test "lookup of sentinel code 'UZ-UNKNOWN' returns UNKNOWN entry" {
    const entry = reg.lookup("UZ-UNKNOWN");
    try std.testing.expectEqualStrings("UZ-UNKNOWN", entry.code);
    try std.testing.expectEqual(std.http.Status.internal_server_error, entry.http_status);
    try std.testing.expectEqualStrings(reg.UNKNOWN.hint, entry.hint);
}

// ── ERR_* constants resolve to correct REGISTRY entries ──────────────────
// Spot-check that ERR_* constant strings match their REGISTRY entries.
// Comptime self-check ensures ALL ERR_* are in REGISTRY; these pin values.

test "ERR_* constants match REGISTRY entry codes (spot check)" {
    // Verify the constant string equals the entry's code field
    try std.testing.expectEqualStrings(reg.ERR_UNAUTHORIZED, reg.lookup(reg.ERR_UNAUTHORIZED).code);
    try std.testing.expectEqualStrings(reg.ERR_AGENTSFLEET_NOT_FOUND, reg.lookup(reg.ERR_AGENTSFLEET_NOT_FOUND).code);
    try std.testing.expectEqualStrings(reg.ERR_EXEC_TIMEOUT_KILL, reg.lookup(reg.ERR_EXEC_TIMEOUT_KILL).code);
    try std.testing.expectEqualStrings(reg.ERR_APPROVAL_CONDITION_INVALID, reg.lookup(reg.ERR_APPROVAL_CONDITION_INVALID).code);
}

// ── Operational hints contain actionable keywords ────────────────────────
// Beyond non-empty, verify key hints have the right operational guidance.

test "startup hints reference 'agentsfleetd doctor' or env vars" {
    const startup_codes = [_][]const u8{
        reg.ERR_STARTUP_ENV_CHECK,
        reg.ERR_STARTUP_CONFIG_LOAD,
        reg.ERR_STARTUP_DB_CONNECT,
        reg.ERR_STARTUP_ENV_ALLOC,
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

// ── Entry struct has exactly 6 fields ─────────────────────────────────────

test "Entry struct has 6 fields (code, http_status, title, hint, docs_uri, user_message)" {
    const fields = @typeInfo(reg.Entry).@"struct".fields;
    try std.testing.expectEqual(@as(usize, 6), fields.len);
}

// ── UNKNOWN sentinel has non-empty fields ─────────────────────────────────

test "UNKNOWN sentinel has all fields populated" {
    try std.testing.expect(reg.UNKNOWN.code.len > 0);
    try std.testing.expect(reg.UNKNOWN.title.len > 0);
    try std.testing.expect(reg.UNKNOWN.hint.len > 0);
    try std.testing.expect(reg.UNKNOWN.docs_uri.len > 0);
    try std.testing.expect(reg.UNKNOWN.user_message == null);
}

// ── user_message ───────────────────────────────────────────────────────

test "an e()-constructed entry (no curated override) has a null user_message" {
    // ERR_PAYLOAD_TOO_LARGE is deliberately NOT one of this spec's 27 curated
    // codes — picked to prove e() (not eu()) still defaults the field to null.
    const entry = reg.lookup(reg.ERR_PAYLOAD_TOO_LARGE);
    try std.testing.expect(entry.user_message == null);
}

test "every code migrated to eu() this spec has a non-empty user_message distinct from its hint" {
    const migrated = [_][]const u8{
        reg.ERR_INTERNAL_DB_UNAVAILABLE,         reg.ERR_INTERNAL_DB_QUERY,
        reg.ERR_INVALID_REQUEST,                 reg.ERR_FORBIDDEN,
        reg.ERR_INSUFFICIENT_SCOPE,              reg.ERR_AGENTSFLEET_NOT_FOUND,
        reg.ERR_FLEET_BUNDLE_INVALID,            reg.ERR_FLEET_BUNDLE_NOT_FOUND,
        reg.ERR_VAULT_DATA_INVALID,              reg.ERR_VAULT_DATA_TOO_LARGE,
        reg.ERR_SECRET_NOT_FOUND,                reg.ERR_PROVIDER_SECRET_REF_REQUIRED,
        reg.ERR_PROVIDER_SECRET_NOT_FOUND,       reg.ERR_PROVIDER_SECRET_DATA_MALFORMED,
        reg.ERR_PROVIDER_MODEL_NOT_IN_CATALOGUE, reg.ERR_APPROVAL_PARSE_FAILED,
        reg.ERR_APPROVAL_NOT_FOUND,              reg.ERR_APPROVAL_INVALID_SIGNATURE,
        reg.ERR_APPROVAL_REDIS_UNAVAILABLE,      reg.ERR_APPROVAL_CONDITION_INVALID,
        reg.ERR_APPROVAL_ALREADY_RESOLVED,       reg.ERR_APIKEY_NOT_FOUND,
        reg.ERR_APIKEY_NAME_TAKEN,               reg.ERR_APIKEY_ALREADY_REVOKED,
        reg.ERR_APIKEY_READONLY_FIELD,           reg.ERR_APIKEY_MUST_REVOKE_FIRST,
        reg.ERR_CRED_INTEGRATION_NOT_CONNECTED,
    };
    try std.testing.expectEqual(@as(usize, 27), migrated.len);
    for (migrated) |code| {
        const entry = reg.lookup(code);
        const um = entry.user_message orelse {
            std.debug.print("code missing user_message: {s}\n", .{code});
            return error.TestExpectedUserMessage;
        };
        try std.testing.expect(um.len > 0);
        try std.testing.expect(!std.mem.eql(u8, um, entry.hint));
    }
}

// The platform-key-missing case (tenant_provider.zig's
// applyPlatform) previously bypassed the registry entirely via a raw
// internalOperationError() string, leaking "operator action required" into a
// dashboard toast. It now has its own dedicated eu()-curated entry rather
// than sharing the generic UZ-INTERNAL-003 bucket (which many unrelated
// internal-failure call sites also use, so one shared user_message couldn't
// fit them all).
test "UZ-PROVIDER-009 (platform key missing) has a curated user_message distinct from its hint" {
    const entry = reg.lookup(reg.ERR_PROVIDER_PLATFORM_KEY_MISSING);
    const um = entry.user_message orelse return error.TestExpectedUserMessage;
    try std.testing.expect(um.len > 0);
    try std.testing.expect(!std.mem.eql(u8, um, entry.hint));
    // Must not leak operator-facing jargon into the dashboard-safe message.
    try std.testing.expect(std.mem.indexOf(u8, um, "operator") == null);
}

// ── Canary: deleted files must not be importable ─────────────────────────
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

test "tenant billing error table validates at comptime" {
    comptime {
        const billing = @import("../state/tenant_billing.zig");
        _ = billing; // comptime validation runs on import
    }
}

test "PgQuery size pinned at 8 bytes (single pointer)" {
    const PgQuery = @import("../db/pg_query.zig").PgQuery;
    try std.testing.expectEqual(@as(usize, 8), @sizeOf(PgQuery));
}

test "FleetSession size pinned at 336 bytes" {
    const FleetSession = @import("../fleet/fleet_session.zig");
    try std.testing.expectEqual(@as(usize, 336), @sizeOf(FleetSession));
}

// ── UZ-PROVIDER-003 hint must match secret_probe.zig's
//    ACTUAL rule (probeSelfManagedSecret): provider + model always required,
//    api_key required for a named provider but OPTIONAL for an openai-compatible
//    endpoint. Regression guard against the old unconditional
//    "provider, api_key, and model (all required)" phrasing that misled clients.
test "UZ-PROVIDER-003 hint states api_key is conditional, not unconditionally required" {
    const hint = reg.lookup(reg.ERR_PROVIDER_SECRET_DATA_MALFORMED).hint;
    // Positive: the conditional rule the validator enforces.
    try std.testing.expect(std.mem.indexOf(u8, hint, "required for a named provider") != null);
    try std.testing.expect(std.mem.indexOf(u8, hint, "optional for an `openai-compatible`") != null);
    // Negative: the old unconditional triplet-required phrasing must be gone.
    try std.testing.expect(std.mem.indexOf(u8, hint, "`provider`, `api_key`, and `model`") == null);
}
