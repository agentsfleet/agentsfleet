//! Public-contract tests for child_exec.zig — the `__execute` child mode.
//!
//! child_exec.run reads its lease off real STDIN and its flags off the real
//! process argv (std.os.argv), and the lease-size guard, exit codes, Landlock
//! fail-closed path, and JSON field extraction are all private to the file.
//! None are reachable from a sibling test without editing child_exec.zig, and
//! run() drives process-global I/O that a unit harness must not touch. What IS
//! a public, load-bearing contract is the argv-flag vocabulary child_exec
//! exports: sandbox_args.buildArgv writes exactly these tokens and main.zig
//! dispatches on SUBCOMMAND, so a rename here silently breaks the parent/child
//! handshake. These tests pin that vocabulary as a regression guard.

const std = @import("std");
const globalIo = @import("common").globalIo;
const child_exec = @import("child_exec.zig");
const sandbox_args = @import("sandbox_args.zig");

test "should expose the __execute subcommand token main dispatches on" {
    // main.zig matches argv[1] against this exact spelling to enter child mode.
    try std.testing.expectEqualStrings("__execute", child_exec.SUBCOMMAND);
}

test "should expose a --workspace= flag prefix that carries a value" {
    // The workspace path (a path, not a secret) rides in argv behind this
    // prefix; child_exec slices the value off after it.
    try std.testing.expectEqualStrings("--workspace=", child_exec.WORKSPACE_FLAG_PREFIX);
    try std.testing.expect(std.mem.endsWith(u8, child_exec.WORKSPACE_FLAG_PREFIX, "="));
}

test "should expose a --sandboxed flag the parent sets for required tiers" {
    // A bare boolean flag (no trailing '='): presence alone signals the child
    // must apply the in-process Landlock policy before reading the lease.
    try std.testing.expectEqualStrings("--sandboxed", child_exec.SANDBOXED_FLAG);
    try std.testing.expect(!std.mem.endsWith(u8, child_exec.SANDBOXED_FLAG, "="));
}

test "should keep the three argv flags mutually distinct" {
    // No flag is a prefix or alias of another — argv parsing in child_exec
    // distinguishes them by exact match (--sandboxed) vs prefix
    // (--workspace=), so an accidental overlap would misroute parsing.
    try std.testing.expect(!std.mem.eql(u8, child_exec.SUBCOMMAND, child_exec.SANDBOXED_FLAG));
    try std.testing.expect(!std.mem.eql(u8, child_exec.SUBCOMMAND, child_exec.WORKSPACE_FLAG_PREFIX));
    try std.testing.expect(!std.mem.startsWith(u8, child_exec.SANDBOXED_FLAG, child_exec.WORKSPACE_FLAG_PREFIX));
    try std.testing.expect(!std.mem.startsWith(u8, child_exec.WORKSPACE_FLAG_PREFIX, child_exec.SANDBOXED_FLAG));
}

test "should let sandbox_args emit a parseable --workspace= value child_exec can slice" {
    // End-to-end on the public surface: the argv sandbox_args builds for the
    // dev_none tier carries the workspace behind child_exec's own prefix, and
    // slicing the prefix off recovers the path verbatim. This is the exact
    // operation child_exec.flagValue performs on argv at runtime.
    const alloc = std.testing.allocator;
    const Config = @import("daemon/config.zig");
    const contract = @import("contract");
    const cfg = Config{
        .control_plane_url = "http://127.0.0.1:8080",
        .runner_token = "agt_rtest",
        .host_id = "host-edge",
        .sandbox_tier = @tagName(contract.protocol.SandboxTier.dev_none),
        .workspace_base = "/tmp/agentsfleet-runner",
        .network_policy = .deny_all_egress,
        .worker_count = 1,
        .cp_deadlines = .{},
        .registry_allowlist = &.{},
        .alloc = alloc,
    };
    const ws = "/tmp/fleet-ws-childexec";
    const argv = try sandbox_args.buildArgv(globalIo(), alloc, cfg, ws, null);
    defer sandbox_args.freeArgv(alloc, argv);

    const ws_flag = argv[argv.len - 1];
    try std.testing.expect(std.mem.startsWith(u8, ws_flag, child_exec.WORKSPACE_FLAG_PREFIX));
    const recovered = ws_flag[child_exec.WORKSPACE_FLAG_PREFIX.len..];
    try std.testing.expectEqualStrings(ws, recovered);
    // The subcommand token sits where main.zig expects it (argv[1] in dev_none).
    try std.testing.expectEqualStrings(child_exec.SUBCOMMAND, argv[1]);
}
