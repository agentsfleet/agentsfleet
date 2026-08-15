//! argv reading for the operator subcommands.
//!
//! These four functions decide where a runner points and which credential it
//! presents, and every one of them had zero executed lines. The cases that
//! matter are the boundaries: a flag in the last argv slot has no value after
//! it, argv[0] is the binary name and must never match a flag, and a missing
//! value must read as "unset" while an allocation failure must not.

const std = @import("std");

const args = @import("args.zig");
const common = @import("common");

const ALLOC = std.testing.allocator;

test "opt returns the entry following the flag" {
    const argv = [_][:0]const u8{ "agentsfleet-runner", "status", "--api", "https://cp.example" };
    try std.testing.expectEqualStrings("https://cp.example", args.opt(&argv, "--api").?);
}

test "opt ignores a trailing flag with no value after it" {
    // `i + 1 < argv.len` is the guard; without it this reads past the end.
    const argv = [_][:0]const u8{ "agentsfleet-runner", "status", "--api" };
    try std.testing.expect(args.opt(&argv, "--api") == null);
}

test "opt returns null for a flag that is absent" {
    const argv = [_][:0]const u8{ "agentsfleet-runner", "status" };
    try std.testing.expect(args.opt(&argv, "--api") == null);
}

test "opt never matches argv[0], which is the binary path" {
    // A runner installed at a path literally named `--api` would otherwise
    // have its own filename read back as a flag value.
    const argv = [_][:0]const u8{ "--api", "https://not-a-flag.example" };
    try std.testing.expect(args.opt(&argv, "--api") == null);
}

test "opt takes the first occurrence when a flag repeats" {
    const argv = [_][:0]const u8{ "runner", "--api", "https://first", "--api", "https://second" };
    try std.testing.expectEqualStrings("https://first", args.opt(&argv, "--api").?);
}

test "has reports a bare flag anywhere after argv[0]" {
    const argv = [_][:0]const u8{ "agentsfleet-runner", "status", "--json" };
    try std.testing.expect(args.has(&argv, "--json"));
    try std.testing.expect(!args.has(&argv, "--verbose"));
}

test "has finds a bare flag in the final slot" {
    const argv = [_][:0]const u8{ "runner", "--json" };
    try std.testing.expect(args.has(&argv, "--json"));
}

test "has never matches argv[0]" {
    const argv = [_][:0]const u8{ "--json", "status" };
    try std.testing.expect(!args.has(&argv, "--json"));
}

test "flagOrEnv prefers the flag over the environment" {
    var map = try common.env.fromPairs(ALLOC, &.{.{ "AGENTSFLEET_API_URL", "https://from-env" }});
    defer map.deinit();
    const argv = [_][:0]const u8{ "runner", "--api", "https://from-flag" };

    const got = (try args.flagOrEnv(&map, &argv, ALLOC, "--api", "AGENTSFLEET_API_URL")).?;
    defer ALLOC.free(got);
    try std.testing.expectEqualStrings("https://from-flag", got);
}

test "flagOrEnv falls back to the environment when the flag is absent" {
    var map = try common.env.fromPairs(ALLOC, &.{.{ "AGENTSFLEET_API_URL", "https://from-env" }});
    defer map.deinit();
    const argv = [_][:0]const u8{ "runner", "status" };

    const got = (try args.flagOrEnv(&map, &argv, ALLOC, "--api", "AGENTSFLEET_API_URL")).?;
    defer ALLOC.free(got);
    try std.testing.expectEqualStrings("https://from-env", got);
}

test "flagOrEnv reports null when neither the flag nor the variable is set" {
    var map = try common.env.fromPairs(ALLOC, &.{});
    defer map.deinit();
    const argv = [_][:0]const u8{ "runner", "status" };

    try std.testing.expect((try args.flagOrEnv(&map, &argv, ALLOC, "--api", "AGENTSFLEET_API_URL")) == null);
}

test "flagOrEnv returns owned memory for the flag path too" {
    // Both branches must be freeable the same way, or callers leak on one of
    // them. Fails under the testing allocator if the flag value is borrowed.
    var map = try common.env.fromPairs(ALLOC, &.{});
    defer map.deinit();
    const argv = [_][:0]const u8{ "runner", "--api", "https://owned" };

    const got = (try args.flagOrEnv(&map, &argv, ALLOC, "--api", "AGENTSFLEET_API_URL")).?;
    defer ALLOC.free(got);
    try std.testing.expectEqualStrings("https://owned", got);
}

test "flagOrEnv propagates allocation failure rather than reading as unset" {
    // The docstring is explicit that OOM must not be masked as "not set" — a
    // runner that silently loses its control-plane URL under memory pressure
    // would fall back to a default endpoint instead of failing.
    var map = try common.env.fromPairs(ALLOC, &.{});
    defer map.deinit();
    const argv = [_][:0]const u8{ "runner", "--api", "https://cp.example" };
    var failing = std.testing.FailingAllocator.init(ALLOC, .{ .fail_index = 0 });

    try std.testing.expectError(
        error.OutOfMemory,
        args.flagOrEnv(&map, &argv, failing.allocator(), "--api", "AGENTSFLEET_API_URL"),
    );
}

test "envOwned returns a duped value and null for an absent name" {
    var map = try common.env.fromPairs(ALLOC, &.{.{ "AGENTSFLEET_RUNNER_TOKEN", "agt_r_probe" }});
    defer map.deinit();

    const got = (try args.envOwned(&map, ALLOC, "AGENTSFLEET_RUNNER_TOKEN")).?;
    defer ALLOC.free(got);
    try std.testing.expectEqualStrings("agt_r_probe", got);

    try std.testing.expect((try args.envOwned(&map, ALLOC, "AGENTSFLEET_ABSENT")) == null);
}
