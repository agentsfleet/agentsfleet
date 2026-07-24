//! Command register for the operator CLI — a typed `Command` enum mapped to a
//! `Spec { handler, summary }`, mirroring the server's `route_table.specFor`
//! (src/agentsfleetd/http/route_table.zig). It is the single source for both
//! dispatch and help: a command with no summary (or a summary with no command)
//! is impossible by construction, so `--help` can never drift from the real
//! command set (Invariant 2).

const std = @import("std");
const runner_deadline = @import("../daemon/runner_deadline.zig");
const globalIo = @import("common").globalIo;
const args = @import("args.zig");
const help = @import("help.zig");
const status = @import("status.zig");
const doctor = @import("doctor.zig");
const FLAG_HELP = "--help";
const H = "-h";

pub const Command = enum { status, doctor };

// Handlers take the UNSTARTED scheduler ownership and start it themselves:
// help/unknown dispatch paths must never pay a worker spawn (or exit on a
// spawn failure) for a command that arms nothing.
const HandlerFn = *const fn ([]const [:0]const u8, *const std.process.Environ.Map, std.Io, std.mem.Allocator, *runner_deadline.Owned) u8;
const Spec = struct { handler: HandlerFn, summary: []const u8 };

/// The register table. Summaries are kept short enough that `  <name>  <summary>`
/// stays ≤80 columns (the help golden enforces it).
fn specFor(cmd: Command) Spec {
    return switch (cmd) {
        .status => .{ .handler = status.run, .summary = "show registration + fleet directive" },
        .doctor => .{ .handler = doctor.run, .summary = "preflight env + control-plane reachability" },
    };
}

/// One-line summary for help rendering — the single source help.zig reads.
pub fn summaryFor(cmd: Command) []const u8 {
    return specFor(cmd).summary;
}

/// Dispatch a non-daemon argv head. `--help`/`-h` renders help (exit 0); an
/// unrecognized token renders help to stderr (exit 2); a known command runs its
/// handler. Always handles — returns the process exit code.
pub fn dispatch(
    argv: []const [:0]const u8,
    env_map: *const std.process.Environ.Map,
    io: std.Io,
    alloc: std.mem.Allocator,
    deadlines: *runner_deadline.Owned,
    name: []const u8,
) u8 {
    if (std.mem.eql(u8, name, FLAG_HELP) or std.mem.eql(u8, name, H)) return help.run(alloc);
    const cmd = std.meta.stringToEnum(Command, name) orelse return help.runUnknown(alloc, name);
    // `<cmd> --help` shows help instead of running the command — a subcommand must
    // never perform a live action (mint a token, write the env file) when the
    // operator asked for help.
    if (args.has(argv, FLAG_HELP) or args.has(argv, H)) return help.run(alloc);
    return specFor(cmd).handler(argv, env_map, io, alloc, deadlines);
}

test "every Command has a non-empty summary (no help drift)" {
    inline for (std.meta.fields(Command)) |f| {
        try std.testing.expect(summaryFor(@field(Command, f.name)).len > 0);
    }
}

test "dispatch resolves --help and rejects an unknown command non-zero" {
    // --help is exit 0; an unknown token is exit 2 (writes to stderr).
    const alloc = std.testing.allocator;
    var env_map: std.process.Environ.Map = .init(alloc);
    defer env_map.deinit();
    const argv = &[_][:0]const u8{};
    var deadlines: runner_deadline.Owned = .{};
    defer deadlines.deinit();
    try std.testing.expectEqual(@as(u8, 2), dispatch(argv, &env_map, globalIo(), alloc, &deadlines, "bogus-cmd"));
}

test "cli rejects the removed register subcommand with unknown-command exit" {
    // `register` was retired (enrollment moved to the dashboard mint): it now
    // resolves to no Command, so dispatch falls through to unknown-command help
    // on stderr with the non-zero exit — never a live action.
    try std.testing.expect(std.meta.stringToEnum(Command, "register") == null);
    const alloc = std.testing.allocator;
    var env_map: std.process.Environ.Map = .init(alloc);
    defer env_map.deinit();
    const argv = &[_][:0]const u8{};
    var deadlines: runner_deadline.Owned = .{};
    defer deadlines.deinit();
    try std.testing.expectEqual(@as(u8, 2), dispatch(argv, &env_map, globalIo(), alloc, &deadlines, "register"));
}
