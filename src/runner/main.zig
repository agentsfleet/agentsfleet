//! `zombie-runner` — host-resident runner binary (keystone skeleton).
//!
//! Pre-claims the build target so the parallel runner streams never collide on
//! build.zig. Today it logs one health line and exits 0, so `zig build
//! zombie-runner` is a green acceptance gate; the event-leasing loop (register
//! → lease → executor → report over loopback to `zombied`) lands in a later
//! slice.

const std = @import("std");
const logging = @import("log");

const log = logging.scoped(.zombie_runner);

// pub: consumed by std at comptime via @import("root").std_options so the
// skeleton's logs are logfmt-shaped (`ts_ms=… level=… scope=… event=… …`),
// matching zombied + the executor sidecar's aggregator pipeline. Without it,
// std falls back to defaultLog and the lines stop being machine-parseable.
pub const std_options: std.Options = .{
    .logFn = runnerLog,
};

fn runnerLog(
    comptime level: std.log.Level,
    comptime scope: @TypeOf(.enum_literal),
    comptime fmt: []const u8,
    args: anytype,
) void {
    const scope_str = comptime if (scope == .default) "default" else @tagName(scope);
    var msg_buf: [2048]u8 = undefined;
    const msg = std.fmt.bufPrint(&msg_buf, fmt, args) catch return;
    var line_buf: [4096]u8 = undefined;
    // Shared envelope helper splices the body and scrubs raw newlines.
    const line = logging.writeLogfmtEnvelope(&line_buf, std.time.milliTimestamp(), @tagName(level), scope_str, msg);
    std.fs.File.stderr().writeAll(line) catch {};
}

pub fn main() void {
    log.info("runner_skeleton_boot", .{ .status = "ok" });
}
