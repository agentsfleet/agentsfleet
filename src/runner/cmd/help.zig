//! `--help` renderer — table-driven off the command register (registry.zig), so
//! the command list cannot drift from what dispatches. Plain ASCII, every line
//! ≤80 columns, no ANSI / no decorative glyphs — the same output contract as
//! `zombiectl`'s help, pinned by a byte-exact golden (test/golden). The version
//! is deliberately NOT in the help body (so the golden is bump-stable);
//! `--version` carries it.

const std = @import("std");
const registry = @import("registry.zig");
const output = @import("output.zig");

const HEADER =
    \\zombie-runner — host-resident runner daemon + operator CLI
    \\
    \\Usage:
    \\  zombie-runner                     run the daemon (bare; reads the env)
    \\  zombie-runner <command> [flags]
    \\  zombie-runner --version | --help
    \\
    \\Commands:
    \\
;

const FLAGS =
    \\
    \\Flags:
    \\  --api <url>      control-plane base URL (else ZOMBIE_API_URL)
    \\  --token <jwt>    admin Clerk JWT for register (else ZOMBIE_TOKEN)
    \\  --host-id <id>   host identifier for register (else RUNNER_HOST_ID)
    \\  --json           machine-readable output (auto when piped)
    \\  --version, -V    print version
    \\  --help, -h       show this help
    \\
    \\Environment:
    \\  ZOMBIE_API_URL        control-plane base URL
    \\  ZOMBIE_TOKEN          platform-admin Clerk JWT (register)
    \\  ZOMBIE_RUNNER_TOKEN   this host's zrn_ token (daemon/status/doctor)
    \\  RUNNER_HOST_ID        stable host identifier (register)
    \\
;

/// Build the full help text. Pure (allocates only the result) so the golden
/// test can compare it byte-for-byte. Command rows come from the register.
pub fn render(alloc: std.mem.Allocator) ![]u8 {
    var list: std.ArrayList(u8) = .{};
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &list);
    const w = &aw.writer;
    try w.writeAll(HEADER);
    inline for (std.meta.fields(registry.Command)) |f| {
        try w.print("  {s:<10} {s}\n", .{ f.name, registry.summaryFor(@field(registry.Command, f.name)) });
    }
    try w.writeAll(FLAGS);
    return aw.toOwnedSlice();
}

/// Render help to stdout (exit 0).
pub fn run(alloc: std.mem.Allocator) u8 {
    const text = render(alloc) catch {
        output.writeOut(HEADER);
        return 0;
    };
    defer alloc.free(text);
    output.writeOut(text);
    return 0;
}

/// Render help to stderr after an "unknown command" line (exit 2).
pub fn runUnknown(alloc: std.mem.Allocator, name: []const u8) u8 {
    var buf: [320]u8 = undefined;
    output.writeErr(std.fmt.bufPrint(&buf, "unknown command: {s}\n\n", .{name}) catch "unknown command\n\n");
    const text = render(alloc) catch return 2;
    defer alloc.free(text);
    output.writeErr(text);
    return 2;
}

test "help body is ≤80 cols, ANSI-free, and lists every command" {
    const alloc = std.testing.allocator;
    const text = try render(alloc);
    defer alloc.free(text);
    var it = std.mem.splitScalar(u8, text, '\n');
    while (it.next()) |line| try std.testing.expect(line.len <= 80);
    try std.testing.expect(std.mem.indexOfScalar(u8, text, 0x1b) == null); // no ESC
    inline for (std.meta.fields(registry.Command)) |f| {
        try std.testing.expect(std.mem.indexOf(u8, text, f.name) != null);
    }
}

// Byte-exact drift guard against the checked-in golden (same contract as
// zombiectl/test/golden/help-no-color.txt). @embedFile is compile-time and
// in-bounds (testdata/ lives under this module's root), so the test needs no
// cwd. Regenerate on an intentional help change:
//   NO_COLOR=1 zig-out/bin/zombie-runner --help > src/runner/cmd/testdata/help.txt
test "help matches the checked-in golden byte-for-byte" {
    const alloc = std.testing.allocator;
    const text = try render(alloc);
    defer alloc.free(text);
    try std.testing.expectEqualStrings(@embedFile("testdata/help.txt"), text);
}
