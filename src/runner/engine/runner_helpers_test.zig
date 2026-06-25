//! Tests for `runner_helpers.zig` — extracted to a sibling file so the source
//! stays under the 350-line cap after the M100 max_tokens hardening landed.
//! Covers the secret-redaction helpers (§1) and the fleet-config int clamp.

const std = @import("std");
const nullclaw = @import("nullclaw");
const Config = nullclaw.config.Config;

const runner_helpers = @import("runner_helpers.zig");
const runner_progress = @import("runner_progress.zig");
const redactedFinalReply = runner_helpers.redactedFinalReply;
const applyFleetConfig = runner_helpers.applyFleetConfig;

test "redactBytes scrubs the lease-delivered provider api_key from a frame" {
    // Invariant: the provider key (now sourced from policy.api_key, captured by
    // collectSecrets as fleet_config.api_key) never reaches an activity frame.
    const alloc = std.testing.allocator;
    const secrets = [_]runner_progress.Secret{
        .{ .value = "fw_live_provider_key", .placeholder = "${secrets.llm.api_key}" },
    };
    const raw = "POST api.fireworks.ai Authorization: Bearer fw_live_provider_key";
    const out = try runner_progress.redactBytes(alloc, raw, &secrets);
    defer if (out.ptr != raw.ptr) alloc.free(out);
    try std.testing.expect(std.mem.indexOf(u8, out, "fw_live_provider_key") == null);
    try std.testing.expect(std.mem.indexOf(u8, out, "${secrets.llm.api_key}") != null);
}

test "redactedFinalReply substitutes the placeholder and frees the input" {
    const alloc = std.testing.allocator;
    const secrets = [_]runner_progress.Secret{
        .{ .value = "sk-leak", .placeholder = "${secrets.llm.api_key}" },
    };
    const input = try alloc.dupe(u8, "hello sk-leak world");
    const out = try redactedFinalReply(alloc, input, &secrets);
    defer alloc.free(out);
    try std.testing.expectEqualStrings("hello ${secrets.llm.api_key} world", out);
}

test "redactedFinalReply with no matching secret still transfers ownership" {
    // Negative-path: when redactBytes returns the input slice unchanged
    // (no hit), the helper must still free `input` and return a fresh
    // copy — caller cannot tell the two paths apart from outside.
    const alloc = std.testing.allocator;
    const secrets = [_]runner_progress.Secret{
        .{ .value = "absent-token", .placeholder = "${secrets.llm.api_key}" },
    };
    const input = try alloc.dupe(u8, "no leak here");
    const out = try redactedFinalReply(alloc, input, &secrets);
    defer alloc.free(out);
    try std.testing.expectEqualStrings("no leak here", out);
    // The std.testing.allocator catches double-free / leak; a defective
    // implementation that returned `input` directly would either leak
    // the dupe or double-free on the caller's defer.
}

test "redactedFinalReply fails closed (no raw leak) when redaction allocation fails" {
    // M100 §1: FailingAllocator index 0 = the response dupe (succeeds), index 1 =
    // redactBytes' internal dupe (fails). The helper must PROPAGATE the error, not
    // fall back to the un-redacted `response` — so a secret never leaves on the
    // terminal reply under memory pressure.
    const secrets = [_]runner_progress.Secret{
        .{ .value = "sk-leak", .placeholder = "${secrets.llm.api_key}" },
    };
    var fa = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 1 });
    const alloc = fa.allocator();
    const response = try alloc.dupe(u8, "hello sk-leak world"); // index 0
    try std.testing.expectError(error.OutOfMemory, redactedFinalReply(alloc, response, &secrets));
}

/// Apply a `{"max_tokens": <v>}` fleet-config and return the resolved cfg field.
fn applyMaxTokens(alloc: std.mem.Allocator, json_body: []const u8) !?u32 {
    var cfg = Config{ .workspace_dir = "", .config_path = "", .allocator = alloc };
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, json_body, .{});
    defer parsed.deinit();
    applyFleetConfig(&cfg, parsed.value);
    return cfg.max_tokens;
}

test "applyFleetConfig clamps out-of-range max_tokens instead of @intCast-panicking (M100)" {
    const alloc = std.testing.allocator;
    // Negative, zero, and >u32max are bad input → ignored (Config default = null),
    // never a panic. A flip back to `@intCast` would crash on the first two.
    try std.testing.expectEqual(@as(?u32, null), try applyMaxTokens(alloc, "{\"max_tokens\": -1}"));
    try std.testing.expectEqual(@as(?u32, null), try applyMaxTokens(alloc, "{\"max_tokens\": 0}"));
    try std.testing.expectEqual(@as(?u32, null), try applyMaxTokens(alloc, "{\"max_tokens\": 4294967296}"));
    try std.testing.expectEqual(@as(?u32, null), try applyMaxTokens(alloc, "{\"max_tokens\": 9999999999999}"));
    // A valid positive value is applied unchanged.
    try std.testing.expectEqual(@as(?u32, 2048), try applyMaxTokens(alloc, "{\"max_tokens\": 2048}"));
    try std.testing.expectEqual(@as(?u32, 4294967295), try applyMaxTokens(alloc, "{\"max_tokens\": 4294967295}"));
}
