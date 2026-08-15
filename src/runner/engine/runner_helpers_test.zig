//! Tests for `runner_helpers.zig` — extracted to a sibling file so the source
//! stays under the 350-line cap after the M100 max_tokens hardening landed.
//! Covers the secret-redaction helpers (§1) and the fleet-config int clamp.

const std = @import("std");
const nullclaw = @import("nullclaw");
const Config = nullclaw.config.Config;

const runner_helpers = @import("runner_helpers.zig");
const runner_progress = @import("runner_progress.zig");
const tools_mod = nullclaw.tools;
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

// ── provider injection (§ensureProviderEntry) ────────────────────────────────
// Neither injector had a single executed line. They are the only path by which
// the lease-delivered key and custom endpoint reach nullclaw's provider table;
// a regression here silently falls back to the process environment — the exact
// trust boundary the injectors exist to close.

test "injectProviderApiKey prepends an entry when the provider table is empty" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var cfg = Config{ .workspace_dir = "", .config_path = "", .allocator = arena.allocator() };
    cfg.default_provider = "openai";

    try runner_helpers.injectProviderApiKey(&cfg, "sk-lease-delivered");

    try std.testing.expectEqual(@as(usize, 1), cfg.providers.len);
    try std.testing.expectEqualStrings("openai", cfg.providers[0].name);
    try std.testing.expectEqualStrings("sk-lease-delivered", cfg.providers[0].api_key.?);
}

test "injectProviderApiKey overwrites the existing entry rather than growing the table" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var cfg = Config{ .workspace_dir = "", .config_path = "", .allocator = arena.allocator() };
    cfg.default_provider = "openai";

    try runner_helpers.injectProviderApiKey(&cfg, "sk-first");
    try runner_helpers.injectProviderApiKey(&cfg, "sk-rotated");

    try std.testing.expectEqual(@as(usize, 1), cfg.providers.len);
    try std.testing.expectEqualStrings("sk-rotated", cfg.providers[0].api_key.?);
}

test "injectProviderBaseUrl lands on the SAME entry as the api key" {
    // The daemon sets default_provider to `custom:<url>`; key and URL must ride
    // one entry or nullclaw dials the custom endpoint without the credential.
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var cfg = Config{ .workspace_dir = "", .config_path = "", .allocator = arena.allocator() };
    cfg.default_provider = "custom:https://llm.internal";

    try runner_helpers.injectProviderApiKey(&cfg, "sk-lease");
    try runner_helpers.injectProviderBaseUrl(&cfg, "https://llm.internal/v1");

    try std.testing.expectEqual(@as(usize, 1), cfg.providers.len);
    try std.testing.expectEqualStrings("sk-lease", cfg.providers[0].api_key.?);
    try std.testing.expectEqualStrings("https://llm.internal/v1", cfg.providers[0].base_url.?);
}

test "applyFleetConfig applies an in-range max_tokens and temperature" {
    // The clamp test above pins the rejects; this pins the accepts — a wire
    // value that IS valid must actually land, or every fleet runs on defaults.
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(
        std.json.Value,
        alloc,
        "{\"temperature\":0.25,\"max_tokens\":2048}",
        .{},
    );
    defer parsed.deinit();

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var cfg = Config{ .workspace_dir = "", .config_path = "", .allocator = arena.allocator() };
    applyFleetConfig(&cfg, parsed.value);

    try std.testing.expectEqual(@as(u32, 2048), cfg.max_tokens.?);
    try std.testing.expectEqual(@as(f64, 0.25), cfg.temperature);
    try std.testing.expectEqual(@as(f64, 0.25), cfg.default_temperature);
}

test "buildToolsFromSpec frees every tool name the bridge could not resolve" {
    // A fleet ships a tools array naming something this runner build does not
    // carry. The bridge hands those names back under `skipped`, and this
    // function owns both the warning an operator reads and the free of every
    // name — a miss here leaks one allocation per unknown tool per lease, and
    // the leak only shows on fleets whose spec has drifted from the build.
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(
        std.json.Value,
        alloc,
        "[\"definitely_not_a_tool\",\"also_not_a_tool\"]",
        .{},
    );
    defer parsed.deinit();

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var cfg = Config{ .workspace_dir = "", .config_path = "", .allocator = arena.allocator() };

    const tools = try runner_helpers.buildToolsFromSpec(
        alloc,
        "/tmp/agentsfleet-tools-spec-test",
        parsed.value,
        &cfg,
        null, // policy — the allTools fallback path
        null, // cred_channel
    );
    defer tools_mod.deinitTools(alloc, tools);

    // Neither name resolves, so the spec contributes no tool. `std.testing.allocator`
    // is the actual assertion: it fails the test if a skipped name went unfreed.
    try std.testing.expectEqual(@as(usize, 0), tools.len);
}

test "a non-array tools spec falls back to the default tool set" {
    // The wire type is `?std.json.Value`; a malformed fleet can send an object
    // or a string. That must degrade to the default set, never error the lease.
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"not\":\"an array\"}", .{});
    defer parsed.deinit();

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var cfg = Config{ .workspace_dir = "", .config_path = "", .allocator = arena.allocator() };

    const tools = try runner_helpers.buildToolsFromSpec(
        alloc,
        "/tmp/agentsfleet-tools-fallback-test",
        parsed.value,
        &cfg,
        null,
        null,
    );
    defer tools_mod.deinitTools(alloc, tools);
    try std.testing.expect(tools.len > 0);
}
