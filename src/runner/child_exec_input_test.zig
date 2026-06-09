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
    // The strongest proof of the fix: at EVERY allocation-failure point the
    // function either returns error.OutOfMemory (checkAllAllocationFailures also
    // asserts no leak) or succeeds with the provider/key pair atomic — present
    // together or absent together. A provider-without-key agent_config (the
    // "wrong provider" hazard) can never escape, even under memory pressure.
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
