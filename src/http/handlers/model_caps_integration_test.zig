//! Integration tests for the model-caps endpoint.
//!
//! Exercises the full HTTP path against a fresh-migrated DB seeded by
//! schema/019_model_caps.sql. Skips gracefully when TEST_DATABASE_URL is unset.

const std = @import("std");
const auth_mw = @import("../../auth/middleware/mod.zig");

const harness_mod = @import("../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const model_caps_h = @import("model_caps.zig");

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
    // Per-token rates accompany every row (zero for BYOK-only models).
    try std.testing.expect(r.bodyContains("\"input_cents_per_mtok\":300"));
    try std.testing.expect(r.bodyContains("\"output_cents_per_mtok\":1500"));
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

    const r = try h.get("/_um/wrong-key/model-caps.json").send();
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
