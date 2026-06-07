//! Integration tests for the model-caps endpoint.
//!
//! Exercises the full HTTP path against a fresh-migrated DB seeded by
//! schema/019_model_caps.sql. Skips gracefully when TEST_DATABASE_URL is unset.

const std = @import("std");
const auth_mw = @import("../../auth/middleware/mod.zig");

const harness_mod = @import("../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const model_caps_h = @import("model_caps.zig");
const tenant_billing = @import("../../state/tenant_billing.zig");

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn openHarnessOrSkip(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
    });
}

test "integration(model_caps): GET returns seed catalogue with claude-sonnet-4-6" {
    const alloc = std.testing.allocator;
    const h = openHarnessOrSkip(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

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
