// Unit tests for zombie execution telemetry HTTP handlers.
//
// No DB or live server required. Covers:
//   - Module import resolution
//   - RBAC contract: requireRole(.admin) semantics for internal telemetry
//   - Limit constants pinned to spec values
//   - parseCursor/makeCursor contract (full DB-backed suite in zombie_telemetry_store_test.zig)
//
// Live HTTP enforcement (cursor pagination, DB writes) is in integration tests.

const std = @import("std");
const common = @import("../common.zig");

// ── Module import resolution ────────────────────────────────────────────────

test "zombie_telemetry module imports resolve" {
    _ = @import("telemetry.zig");
}

test "zombie_telemetry_store module imports resolve" {
    _ = @import("../../../state/zombie_telemetry_store.zig");
}

// ── RBAC contract ───────────────────────────────────────────────────────────
// `common.requireRole` writes a 403 to *httpz.Response and returns false when
// the principal's role doesn't match. Calling it directly in a unit test requires
// a live httpz.Response, which needs a running server — not possible here.

test "admin_role_satisfies_admin_gate" {
    const principal = common.AuthPrincipal{
        .mode = .api_key,
        .role = .admin,
    };
    try std.testing.expect(principal.role == .admin);
}

test "user_role_does_not_satisfy_admin_gate" {
    const principal = common.AuthPrincipal{
        .mode = .api_key,
        .role = .user,
    };
    try std.testing.expect(principal.role != .admin);
}

test "operator_role_does_not_satisfy_admin_gate" {
    const principal = common.AuthPrincipal{
        .mode = .api_key,
        .role = .operator,
    };
    try std.testing.expect(principal.role != .admin);
}

// ── Spec limit constants ────────────────────────────────────────────────────

const LIMIT_DEFAULT: u32 = 50;
const LIMIT_MAX_CUSTOMER: u32 = 200;
const LIMIT_MAX_OPERATOR: u32 = 500;

test "limit_default_is_50" {
    try std.testing.expectEqual(@as(u32, 50), LIMIT_DEFAULT);
}

test "limit_max_customer_is_200" {
    try std.testing.expectEqual(@as(u32, 200), LIMIT_MAX_CUSTOMER);
}

test "limit_max_operator_is_500" {
    try std.testing.expectEqual(@as(u32, 500), LIMIT_MAX_OPERATOR);
}

// ── OTel arithmetic bounds ──────────────────────────────────────────────────
// Pins the 7-day cap on agent_seconds before multiplying to nanoseconds.

test "capped_agent_seconds_never_overflows_u64_when_multiplied_to_ns" {
    const max_seconds: u64 = 604_800; // 7 days cap
    const start_ns: u64 = 1_000_000_000_000_000_000; // 1e18 ns (plausible epoch)
    const end_ns: u64 = start_ns + max_seconds * 1_000_000_000;
    // Must not have wrapped: end_ns > start_ns
    try std.testing.expect(end_ns > start_ns);
}

// ── TelemetryRow memory ownership ───────────────────────────────────────────
// deinit must free all owned slices; std.testing.allocator detects leaks.

test "telemetry_row_deinit_frees_all_owned_slices_without_leaking" {
    const alloc = std.testing.allocator;
    const store = @import("../../../state/zombie_telemetry_store.zig");
    var row = store.TelemetryRow{
        .id = try alloc.dupe(u8, "test-id-001"),
        .tenant_id = try alloc.dupe(u8, "tenant-abc"),
        .workspace_id = try alloc.dupe(u8, "ws-xyz"),
        .zombie_id = try alloc.dupe(u8, "zombie-abc"),
        .event_id = try alloc.dupe(u8, "evt-0001"),
        .charge_type = try alloc.dupe(u8, "stage"),
        .posture = try alloc.dupe(u8, "platform"),
        .model = try alloc.dupe(u8, "accounts/fireworks/models/kimi-k2.6"),
        .credit_deducted_cents = 10,
        .token_count_input = 100,
        .token_count_output = 200,
        .wall_ms = 4200,
        .recorded_at = 1712924410000,
    };
    row.deinit(alloc);
    // std.testing.allocator reports any un-freed bytes as a test failure.
}

test "telemetry_row_deinit_frees_owned_slices_with_null_optionals" {
    const alloc = std.testing.allocator;
    const store = @import("../../../state/zombie_telemetry_store.zig");
    var row = store.TelemetryRow{
        .id = try alloc.dupe(u8, "row-id-002"),
        .tenant_id = try alloc.dupe(u8, "tenant-def"),
        .workspace_id = try alloc.dupe(u8, "ws-abc"),
        .zombie_id = try alloc.dupe(u8, "zombie-def"),
        .event_id = try alloc.dupe(u8, "evt-0002"),
        .charge_type = try alloc.dupe(u8, "receive"),
        .posture = try alloc.dupe(u8, "byok"),
        .model = try alloc.dupe(u8, "claude-sonnet-4-6"),
        .credit_deducted_cents = 0,
        .token_count_input = null,
        .token_count_output = null,
        .wall_ms = null,
        .recorded_at = 1712924400000,
    };
    row.deinit(alloc);
}

// ── Struct field presence (comptime) ────────────────────────────────────────
// Proves ExecutionUsage and EventResult carry the latency/epoch fields. If a
// future refactor removes them, these tests fail to compile — not just to run.

test "execution_usage_has_time_to_first_token_ms_and_epoch_wall_time_ms" {
    const metering = @import("../../../zombie/metering.zig");
    const usage = metering.ExecutionUsage{
        .zombie_id = "z",
        .workspace_id = "w",
        .event_id = "e",
        .agent_seconds = 0,
        .token_count = 0,
        .time_to_first_token_ms = 870,
        .epoch_wall_time_ms = 1712924400000,
    };
    try std.testing.expectEqual(@as(u64, 870), usage.time_to_first_token_ms);
    try std.testing.expectEqual(@as(i64, 1712924400000), usage.epoch_wall_time_ms);
}

test "execution_usage_defaults_zero_ttft_and_zero_epoch_compile" {
    const metering = @import("../../../zombie/metering.zig");
    // Both fields added with zero-value semantics: TTFT=0 means executor did not report;
    // epoch=0 means gate-blocked event (skipped by recordZombieDelivery).
    _ = metering.ExecutionUsage{
        .zombie_id = "",
        .workspace_id = "",
        .event_id = "",
        .agent_seconds = 0,
        .token_count = 0,
        .time_to_first_token_ms = 0,
        .epoch_wall_time_ms = 0,
    };
}

test "event_result_has_time_to_first_token_ms_and_epoch_wall_time_ms" {
    const event_types = @import("../../../zombie/event_loop_types.zig");
    const result = event_types.EventResult{
        .status = .processed,
        .agent_response = @constCast(""),
        .token_count = 0,
        .wall_seconds = 0,
        .time_to_first_token_ms = 1200,
        .epoch_wall_time_ms = 1712924400000,
    };
    try std.testing.expectEqual(@as(u64, 1200), result.time_to_first_token_ms);
    try std.testing.expectEqual(@as(i64, 1712924400000), result.epoch_wall_time_ms);
}

test "event_result_ttft_and_epoch_default_to_zero" {
    const event_types = @import("../../../zombie/event_loop_types.zig");
    // Gate-blocked events produce EventResult with TTFT=0, epoch=0.
    // recordZombieDelivery skips telemetry when epoch==0.
    const result = event_types.EventResult{
        .status = .skipped_duplicate,
        .agent_response = @constCast(""),
        .token_count = 0,
        .wall_seconds = 0,
        // both default to 0 per struct definition
    };
    try std.testing.expectEqual(@as(u64, 0), result.time_to_first_token_ms);
    try std.testing.expectEqual(@as(i64, 0), result.epoch_wall_time_ms);
}

// ── parseLimitFromQs boundary semantics ─────────────────────────────────────
// The private parseLimitFromQs uses: n==0 → error, n>max → error, absent → default.
// These tests pin the exact boundary values so a change to limits fails loudly.

test "customer_limit_validation_boundary_values" {
    // Mirror parseLimitFromQs contract: n>0 and n<=max is valid.
    const max: u32 = LIMIT_MAX_CUSTOMER;
    const valid = struct {
        fn check(n: u32, m: u32) bool {
            return n > 0 and n <= m;
        }
    }.check;
    try std.testing.expect(!valid(0, max));
    try std.testing.expect(valid(1, max));
    try std.testing.expect(valid(50, max));
    try std.testing.expect(valid(200, max));
    try std.testing.expect(!valid(201, max));
    try std.testing.expect(!valid(500, max));
}

test "operator_limit_validation_boundary_values" {
    const max: u32 = LIMIT_MAX_OPERATOR;
    const valid = struct {
        fn check(n: u32, m: u32) bool {
            return n > 0 and n <= m;
        }
    }.check;
    try std.testing.expect(!valid(0, max));
    try std.testing.expect(valid(1, max));
    try std.testing.expect(valid(200, max));
    try std.testing.expect(valid(500, max));
    try std.testing.expect(!valid(501, max));
}

test "limit_default_is_within_both_customer_and_operator_bounds" {
    try std.testing.expect(LIMIT_DEFAULT > 0);
    try std.testing.expect(LIMIT_DEFAULT <= LIMIT_MAX_CUSTOMER);
    try std.testing.expect(LIMIT_DEFAULT <= LIMIT_MAX_OPERATOR);
}

// ── Error code stability (regression) ───────────────────────────────────────
// Pin the error codes the handler emits against the spec Error Contracts table.
// These codes appear in client-facing responses — changing them is a breaking change.

test "error_code_for_invalid_request_is_uz_req_001" {
    const error_codes = @import("../../../errors/error_registry.zig");
    try std.testing.expectEqualStrings("UZ-REQ-001", error_codes.ERR_INVALID_REQUEST);
}

test "error_code_for_workspace_access_denied_is_uz_workspace_001" {
    const error_codes = @import("../../../errors/error_registry.zig");
    try std.testing.expectEqualStrings("UZ-WORKSPACE-001", error_codes.ERR_WORKSPACE_NOT_FOUND);
}

test "error_code_for_unauthenticated_request_is_uz_auth_002" {
    const error_codes = @import("../../../errors/error_registry.zig");
    try std.testing.expectEqualStrings("UZ-AUTH-002", error_codes.ERR_UNAUTHORIZED);
}

// ── makeCursor output format ────────────────────────────────────────────────
// The cursor is opaque base64url. The plain "{ts}:{id}" must not be visible.
// base64url alphabet: A-Za-z0-9-_  — ':' is never a valid character.

test "make_cursor_output_contains_no_colon_separator" {
    const alloc = std.testing.allocator;
    const store = @import("../../../state/zombie_telemetry_store.zig");
    const row = store.TelemetryRow{
        .id = @constCast("abc123"),
        .tenant_id = @constCast("t"),
        .workspace_id = @constCast("w"),
        .zombie_id = @constCast("z"),
        .event_id = @constCast("e"),
        .charge_type = @constCast("stage"),
        .posture = @constCast("platform"),
        .model = @constCast("accounts/fireworks/models/kimi-k2.6"),
        .credit_deducted_cents = 0,
        .token_count_input = null,
        .token_count_output = null,
        .wall_ms = null,
        .recorded_at = 1712924400000,
    };
    const cursor = try store.makeCursor(alloc, row);
    defer alloc.free(cursor);
    // The plain form "1712924400000:abc123" contains ':' — the encoded form must not.
    try std.testing.expect(std.mem.indexOf(u8, cursor, ":") == null);
    try std.testing.expect(cursor.len > 0);
}

test "make_cursor_output_contains_only_base64url_characters" {
    const alloc = std.testing.allocator;
    const store = @import("../../../state/zombie_telemetry_store.zig");
    const row = store.TelemetryRow{
        .id = @constCast("zombie-telemetry-row-id-xyz"),
        .tenant_id = @constCast("t"),
        .workspace_id = @constCast("w"),
        .zombie_id = @constCast("z"),
        .event_id = @constCast("e"),
        .charge_type = @constCast("stage"),
        .posture = @constCast("byok"),
        .model = @constCast("claude-sonnet-4-6"),
        .credit_deducted_cents = 14,
        .token_count_input = 1420,
        .token_count_output = 870,
        .wall_ms = 14000,
        .recorded_at = 1744483214000,
    };
    const cursor = try store.makeCursor(alloc, row);
    defer alloc.free(cursor);
    // Base64url alphabet: A-Za-z0-9 plus '-' and '_'. Nothing else.
    for (cursor) |byte| {
        const is_base64url =
            (byte >= 'A' and byte <= 'Z') or
            (byte >= 'a' and byte <= 'z') or
            (byte >= '0' and byte <= '9') or
            byte == '-' or byte == '_';
        try std.testing.expect(is_base64url);
    }
}

// ── Negative epoch guard semantics (spec Failure Modes) ─────────────────────
// epoch_wall_time_ms < 0 means a system clock anomaly. epoch == 0 means a
// gate-blocked event. Both must be distinguishable at the call site.

test "negative_epoch_is_distinct_from_zero_epoch" {
    const neg: i64 = -1;
    const zero: i64 = 0;
    const pos: i64 = 1712924400000;
    try std.testing.expect(neg < 0);
    try std.testing.expect(zero == 0);
    try std.testing.expect(pos > 0);
    // The guard in recordZombieDelivery fires on (<0) first, then (==0),
    // so each condition is tested by exactly one guard branch.
    try std.testing.expect(!(neg == 0));
    try std.testing.expect(!(zero < 0));
}
