//! Integration tests for the model-caps endpoint.
//!
//! The catalogue ships EMPTY (migration 019 no longer seeds it; platform admins
//! populate it via /v1/admin/models). These tests self-seed the rows they assert
//! on through the DB, so they prove the public read path without depending on a
//! frozen migration seed. Skips gracefully when TEST_DATABASE_URL is unset.

const std = @import("std");
const clock = @import("common").clock;
const auth_mw = @import("../../auth/middleware/mod.zig");

const harness_mod = @import("../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const model_caps_h = @import("model_caps.zig");
const tenant_billing = @import("../../state/tenant_billing.zig");

// uuidv7 literals (version nibble 7) so the model_caps uid CHECK passes.
const UID_SONNET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a8001";
const UID_KIMI = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a8002";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn openHarnessOrSkip(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
    });
}

// Seed the two rows the read-path tests assert on. Rates mirror the old 019
// seed (Sonnet $3/M in · $15/M out in nanos) so the wire-shape assertions below
// pin the same values, now sourced from a test-owned insert.
fn seedCaps(h: *TestHarness) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO core.model_caps
        \\  (uid, model_id, provider, context_cap_tokens, input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at_ms, updated_at_ms)
        \\VALUES ($1::uuid, 'claude-sonnet-4-6', 'anthropic', 256000, 3000000000, 300000000, 15000000000, $3, $3),
        \\       ($2::uuid, 'kimi-k2.6', 'moonshot', 256000, 0, 0, 0, $3, $3)
        \\ON CONFLICT (provider, model_id) DO NOTHING
    , .{ UID_SONNET, UID_KIMI, now });
}

fn cleanupCaps(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    _ = conn.exec("DELETE FROM core.model_caps WHERE uid IN ($1::uuid, $2::uuid)", .{ UID_SONNET, UID_KIMI }) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

test "integration(model_caps): GET returns the catalogue with claude-sonnet-4-6" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try seedCaps(h);
    defer cleanupCaps(h);

    const r = try h.get(model_caps_h.MODEL_CAPS_PATH).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"version\""));
    try std.testing.expect(r.bodyContains("claude-sonnet-4-6"));
    try std.testing.expect(r.bodyContains("kimi-k2.6"));
    try std.testing.expect(r.bodyContains("\"context_cap_tokens\":256000"));
    // Per-token rates accompany every row (zero for self-managed-only models).
    // Sonnet rates: $3/Mtok input · $15/Mtok output, expressed in nanos
    // (1 nano = 1/1B USD; cents → nanos × 10M).
    try std.testing.expect(r.bodyContains("\"input_nanos_per_mtok\":3000000000"));
    try std.testing.expect(r.bodyContains("\"output_nanos_per_mtok\":15000000000"));
}

test "integration(model_caps): GET ?model=<known> returns one row" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try seedCaps(h);
    defer cleanupCaps(h);

    const r = try h.get(model_caps_h.MODEL_CAPS_PATH ++ "?model=claude-sonnet-4-6").send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("claude-sonnet-4-6"));
    try std.testing.expect(r.bodyContains("\"context_cap_tokens\":256000"));
    // Other models should NOT appear in a filtered response.
    try std.testing.expect(!r.bodyContains("kimi-k2.6"));
}

test "integration(model_caps): GET ?model=<unknown> returns 200 with empty array" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const r = try h.get(model_caps_h.MODEL_CAPS_PATH ++ "?model=does-not-exist").send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"models\":[]"));
}

test "integration(model_caps): wrong key returns 404" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const r = try h.get("/_um/wrong-key/cap.json").send();
    defer r.deinit();
    try r.expectStatus(.not_found);
}

test "integration(model_caps): POST returns 405" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const req = try h.post(model_caps_h.MODEL_CAPS_PATH).json("{}");
    const r = try req.send();
    defer r.deinit();
    try r.expectStatus(.method_not_allowed);
}

test "integration(cap_json): global rates + billing block matches billing constants" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const r = try h.get(model_caps_h.MODEL_CAPS_PATH).send();
    defer r.deinit();
    try r.expectStatus(.ok);

    // Each global field is rendered from the same constants the billing math
    // reads, so the public document cannot drift from the enforcer.
    const cfg = tenant_billing.publicConfig();
    var buf: [96]u8 = undefined;
    try std.testing.expect(r.bodyContains(try std.fmt.bufPrint(&buf, "\"run_nanos_per_sec\":{d}", .{cfg.run_nanos_per_sec})));
    try std.testing.expect(r.bodyContains(try std.fmt.bufPrint(&buf, "\"event_nanos\":{d}", .{cfg.event_nanos})));
    try std.testing.expect(r.bodyContains(try std.fmt.bufPrint(&buf, "\"starter_credit_nanos\":{d}", .{cfg.starter_credit_nanos})));
    try std.testing.expect(r.bodyContains(try std.fmt.bufPrint(&buf, "\"free_trial_end_ms\":{d}", .{cfg.free_trial_end_ms})));
    try std.testing.expect(r.bodyContains(try std.fmt.bufPrint(&buf, "\"free_trial_stage_nanos\":{d}", .{cfg.free_trial_stage_nanos})));
}
