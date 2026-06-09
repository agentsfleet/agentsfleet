//! Tests for `child_exec_input.zig` — the engine-input assembly (`buildCallArgs`
//! / `buildInstructionsContext`). Split out of `child_exec.zig` to keep that file
//! under the RULE FLL line limit; the `runEngine` fail-closed tests stay there
//! because they exercise that file's private `runEngine`.

const std = @import("std");
const testing = std.testing;

const input = @import("child_exec_input.zig");
const wire = @import("engine/wire.zig");
const testLease = @import("child_exec_test_fixtures.zig").testLease;

test "buildCallArgs injects the policy provider and api_key into agent_config" {
    const alloc = testing.allocator;
    const payload = testLease(.{ .provider = "fireworks", .api_key = "fw_secret_key" });
    var args = try input.buildCallArgs(alloc, payload);
    defer args.deinit(alloc);
    const ac = args.agent_config.?.object;
    try testing.expectEqualStrings("fireworks", ac.get(wire.provider).?.string);
    try testing.expectEqualStrings("fw_secret_key", ac.get(wire.api_key).?.string);
}

test "buildInstructionsContext attaches the installed instructions under the wire key" {
    const alloc = testing.allocator;
    var ctx = try input.buildInstructionsContext(alloc, "do platform ops");
    defer ctx.deinit(alloc);
    try testing.expectEqualStrings("do platform ops", ctx.get(wire.installed_instructions).?.string);
}

test "buildInstructionsContext leaks nothing on allocation failure (every alloc site)" {
    // checkAllAllocationFailures fails each allocation site in turn and asserts
    // the function returns error.OutOfMemory and frees everything (the errdefer
    // is correct) — the canonical Zig zero-leak proof for the error path.
    try testing.checkAllAllocationFailures(testing.allocator, struct {
        fn run(a: std.mem.Allocator) !void {
            var ctx = try input.buildInstructionsContext(a, "do platform ops");
            ctx.deinit(a);
        }
    }.run, .{});
}

test "buildCallArgs treats an llm-named tool secret as a tool secret, not the provider key" {
    const alloc = testing.allocator;
    // The retired heuristic used to pull the provider key from secrets_map["llm"].
    // A tool secret literally named `llm` must now be left alone.
    var sm = try std.json.parseFromSlice(std.json.Value, alloc, "{\"llm\":{\"api_key\":\"sk-should-not-leak\"}}", .{});
    defer sm.deinit();
    const payload = testLease(.{ .secrets_map = sm.value, .context = .{ .model = "claude-x" } });
    var args = try input.buildCallArgs(alloc, payload);
    defer args.deinit(alloc);
    const ac = args.agent_config.?.object;
    try testing.expectEqualStrings("claude-x", ac.get(wire.model).?.string); // agent_config is populated…
    try testing.expect(ac.get(wire.api_key) == null); // …but the llm tool secret is NOT promoted to the provider key
    try testing.expect(ac.get(wire.provider) == null);
}

test "buildCallArgs injects neither half of an incomplete provider key pair" {
    const alloc = testing.allocator;
    // api_key present, provider empty — a malformed lease. Inject nothing so the
    // engine fails to authenticate cleanly rather than running the wrong provider.
    const payload = testLease(.{ .api_key = "fw_orphan_key", .context = .{ .model = "claude-x" } });
    var args = try input.buildCallArgs(alloc, payload);
    defer args.deinit(alloc);
    const ac = args.agent_config.?.object;
    try testing.expect(ac.get(wire.api_key) == null);
    try testing.expect(ac.get(wire.provider) == null);
}

test "buildCallArgs leaks nothing on allocation failure (every alloc site)" {
    // checkAllAllocationFailures fails each allocation site in turn and asserts
    // the function returns error.OutOfMemory and frees everything — the canonical
    // zero-leak proof for the error path. The fixture exercises every allocating
    // branch: model put, the provider/key pair, the tools array, and the request
    // JSON parse, so each site's errdefer is verified.
    try testing.checkAllAllocationFailures(testing.allocator, struct {
        fn run(a: std.mem.Allocator) !void {
            const payload = testLease(.{
                .provider = "fireworks",
                .api_key = "fw_secret_key",
                .tools = &.{ "bash", "read", "write" },
                .context = .{ .model = "claude-x" },
            });
            var args = try input.buildCallArgs(a, payload);
            args.deinit(a);
        }
    }.run, .{});
}

test "buildCallArgs never yields a half-built provider/key pair under OOM (atomic at every alloc site)" {
    // Two proofs combine here. (1) The success-path assertion below: whenever the
    // build succeeds, provider and api_key are present together or absent together.
    // (2) The rollback under OOM — if the api_key `put` fails after the provider
    // `put` succeeded, the function returns error.OutOfMemory (the `try` below
    // propagates it before the `if`) and the errdefer frees the partial map — is
    // proven by checkAllAllocationFailures asserting zero leak at every alloc site.
    // Together: a provider-without-key agent_config (the "wrong provider" hazard)
    // can never escape, even under memory pressure.
    try testing.checkAllAllocationFailures(testing.allocator, struct {
        fn run(a: std.mem.Allocator) !void {
            const payload = testLease(.{
                .provider = "fireworks",
                .api_key = "fw_secret_key",
                .context = .{ .model = "claude-x" },
            });
            var args = try input.buildCallArgs(a, payload);
            defer args.deinit(a);
            if (args.agent_config) |cfg| {
                const has_provider = cfg.object.get(wire.provider) != null;
                const has_key = cfg.object.get(wire.api_key) != null;
                try testing.expectEqual(has_provider, has_key);
            }
        }
    }.run, .{});
}

test "buildCallArgs assembles a complete engine config from a production-shaped lease" {
    const alloc = testing.allocator;
    // The integration seam the engine consumes: a full lease (model + atomic
    // provider/key + tools + a request carrying a message) must produce an
    // agent_config and tools_spec that carry every field intact — a regression
    // guard so the fail-closed refactor never silently drops a field on success.
    const payload = testLease(.{
        .provider = "fireworks",
        .api_key = "fw_secret_key",
        .tools = &.{ "bash", "read", "write" },
        .context = .{ .model = "claude-x" },
    });
    var args = try input.buildCallArgs(alloc, payload);
    defer args.deinit(alloc);

    const ac = args.agent_config.?.object;
    try testing.expectEqualStrings("claude-x", ac.get(wire.model).?.string);
    try testing.expectEqualStrings("fireworks", ac.get(wire.provider).?.string);
    try testing.expectEqualStrings("fw_secret_key", ac.get(wire.api_key).?.string);
    try testing.expectEqual(@as(usize, 3), args.tools_spec.?.array.items.len);
    try testing.expectEqualStrings("hi", args.message.?); // resolved from the event's "message" field
}

// ── message-resolution fallback branches ─────────────────────────────────────
// Each case drives one fall-through in the `message` blk; without these the only
// covered path is "valid object with a string message field" and a mutant on any
// fallback survives. The fixture's request_json is overridden per case.

fn leaseWithBody(body: []const u8) @TypeOf(testLease(.{})) {
    var payload = testLease(.{ .context = .{ .model = "m" } });
    payload.event.request_json = body;
    return payload;
}

test "buildCallArgs falls back to the raw body as the message when request JSON is malformed" {
    const alloc = testing.allocator;
    // Malformed JSON → parseFromSlice returns a syntax error (NOT OutOfMemory),
    // so the `else => null` arm runs and the message is the raw body. This is the
    // ONLY test that exercises that arm — checkAllAllocationFailures cannot, since
    // it only ever induces OutOfMemory at the parse site.
    var args = try input.buildCallArgs(alloc, leaseWithBody("{not valid json"));
    defer args.deinit(alloc);
    try testing.expectEqualStrings("{not valid json", args.message.?);
}

test "buildCallArgs uses the raw body as the message when no message field is present" {
    const alloc = testing.allocator;
    var args = try input.buildCallArgs(alloc, leaseWithBody("{\"action\":\"push\"}"));
    defer args.deinit(alloc);
    try testing.expectEqualStrings("{\"action\":\"push\"}", args.message.?);
}

test "buildCallArgs uses the raw body when the message field is not a string" {
    const alloc = testing.allocator;
    var args = try input.buildCallArgs(alloc, leaseWithBody("{\"message\":42}"));
    defer args.deinit(alloc);
    try testing.expectEqualStrings("{\"message\":42}", args.message.?);
}

test "buildCallArgs uses the raw body when the request JSON is not an object" {
    const alloc = testing.allocator;
    var args = try input.buildCallArgs(alloc, leaseWithBody("[1,2,3]"));
    defer args.deinit(alloc);
    try testing.expectEqualStrings("[1,2,3]", args.message.?);
}

test "buildCallArgs yields null agent_config and tools_spec for an empty policy" {
    const alloc = testing.allocator;
    // No model, no provider/key, no tools → the `count() > 0`/`items.len > 0`
    // guards take their null side; a mutant that wraps an empty object/array
    // (e.g. `if (true)`) is caught here.
    var args = try input.buildCallArgs(alloc, testLease(.{}));
    defer args.deinit(alloc);
    try testing.expect(args.agent_config == null);
    try testing.expect(args.tools_spec == null);
    try testing.expectEqualStrings("hi", args.message.?); // message still resolves
}

test "buildCallArgs leaks nothing under OOM even when the request JSON is malformed" {
    // The malformed-body path has a different alloc/free shape (parse fails, no
    // req_parsed arena to deinit). Prove it is leak-clean at every alloc-failure
    // index too, so the `else => null` arm never strands memory under pressure.
    try testing.checkAllAllocationFailures(testing.allocator, struct {
        fn run(a: std.mem.Allocator) !void {
            var payload = testLease(.{ .provider = "fireworks", .api_key = "fw_key", .context = .{ .model = "m" } });
            payload.event.request_json = "{not valid json";
            var args = try input.buildCallArgs(a, payload);
            args.deinit(a);
        }
    }.run, .{});
}
