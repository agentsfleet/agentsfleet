//! Post-migration source audit: the deadline-scheduler cutover.
//!
//! Dimensions 4.1 and the metrics row are claims about what production source
//! no longer contains: no per-instance watchdog type, no public raw-socket
//! shutdown, no `socketHandle()` deadline seam, no caller-owned descriptor arm,
//! and no deadline log line carrying a descriptor or credential. Those are
//! properties of the tree, not of a running object, so they are asserted by
//! reading the tree. A behavioural test cannot prove the ABSENCE of a second
//! mechanism; a grep can.
//!
//! CWD during `zig build` is the workspace root, matching the repository's
//! other source-audit sweeps (`internal_op_error_sweep_test.zig`).

const std = @import("std");
const common = @import("common");

/// Roots that must be free of the retired surface. Test files are included:
/// a test that still constructs a watchdog would mean the type still exists.
const PRODUCTION_ROOTS = [_][]const u8{ "src/agentsfleetd", "src/runner", "src/lib" };

/// Generous ceiling for one Zig source file.
const MAX_SOURCE_BYTES: usize = 512 * 1024;

/// This audit file names every retired symbol in order to search for it, so it
/// would match itself on every rule. It is the one file excluded.
const SELF_PATH_SUFFIX = "migration_audit_test.zig";

/// A retired surface plus the reason it is gone, so a failure explains itself
/// instead of printing a bare needle.
const RetiredSurface = struct {
    needle: []const u8,
    why: []const u8,
};

const RETIRED: []const RetiredSurface = &.{
    .{
        .needle = "call_deadline.Watchdog(",
        .why = "per-instance watchdog thread — every caller arms the one ProcessScheduler",
    },
    .{
        .needle = "CallWatchdog",
        .why = "per-client watchdog instantiation — replaced by a stack-local SocketOwner",
    },
    .{
        .needle = "pub fn shutdownSocket",
        .why = "public raw-socket shutdown — cancellation goes through an owner, never a descriptor",
    },
    .{
        .needle = "socketHandle()",
        .why = "raw-descriptor deadline seam — a target exposes a generation, not a socket",
    },
};

/// Log field names that must never ride a deadline event. A descriptor is not
/// an identity and a credential is never an operation label.
const FORBIDDEN_LOG_FIELDS = [_][]const u8{ ".handle =", ".fd =", ".socket =", ".token =", ".password =", ".authorization =" };

/// Every file that emits a deadline event. Kept explicit rather than discovered
/// so a new emitter must be added here consciously.
const DEADLINE_EVENT_SOURCES = [_][]const u8{
    "src/lib/call_deadline/scheduler.zig",
    "src/runner/daemon/control_plane_deadline.zig",
    "src/agentsfleetd/http/handlers/connectors/bounded_fetch.zig",
};

/// Read one source file into `alloc`.
fn readSource(alloc: std.mem.Allocator, path: []const u8) ![]u8 {
    return std.Io.Dir.cwd().readFileAlloc(common.globalIo(), path, alloc, .limited(MAX_SOURCE_BYTES));
}

/// Walk `root` and hand every `.zig` file's contents to `check`.
fn sweepZigFiles(
    alloc: std.mem.Allocator,
    root: []const u8,
    context: anytype,
    comptime check: fn (@TypeOf(context), path: []const u8, source: []const u8) anyerror!void,
) !void {
    const io = common.globalIo();
    var dir = try std.Io.Dir.cwd().openDir(io, root, .{ .iterate = true });
    defer dir.close(io);

    var walker = try dir.walk(alloc);
    defer walker.deinit();

    while (try walker.next(io)) |entry| {
        if (entry.kind != .file) continue;
        if (!std.mem.endsWith(u8, entry.basename, ".zig")) continue;
        if (std.mem.endsWith(u8, entry.path, SELF_PATH_SUFFIX)) continue;

        const full = try std.fmt.allocPrint(alloc, "{s}/{s}", .{ root, entry.path });
        defer alloc.free(full);
        const source = try readSource(alloc, full);
        defer alloc.free(source);
        try check(context, full, source);
    }
}

const RetiredHit = struct {
    found: bool = false,

    fn check(self: *RetiredHit, path: []const u8, source: []const u8) anyerror!void {
        for (RETIRED) |retired| {
            if (std.mem.indexOf(u8, source, retired.needle) != null) {
                std.debug.print(
                    "FAIL: {s} still references `{s}` — {s}\n",
                    .{ path, retired.needle, retired.why },
                );
                self.found = true;
            }
        }
    }
};

test "test_deadline_migration_has_no_raw_fd_surface" {
    const alloc = std.testing.allocator;
    var hit: RetiredHit = .{};
    for (PRODUCTION_ROOTS) |root| try sweepZigFiles(alloc, root, &hit, RetiredHit.check);
    try std.testing.expect(!hit.found);
}

test "test_deadline_events_are_structured_and_redacted" {
    const alloc = std.testing.allocator;
    for (DEADLINE_EVENT_SOURCES) |path| {
        const source = try readSource(alloc, path);
        defer alloc.free(source);

        // Every emitter logs through the scoped logfmt logger, never a bare
        // print — structure is what makes these events queryable.
        try std.testing.expect(std.mem.indexOf(u8, source, "logging.scoped(") != null);
        try std.testing.expect(std.mem.indexOf(u8, source, "std.debug.print") == null);

        for (FORBIDDEN_LOG_FIELDS) |field| {
            if (std.mem.indexOf(u8, source, field) != null) {
                std.debug.print(
                    "FAIL: {s} puts `{s}` on a log line — a deadline event carries no descriptor or credential\n",
                    .{ path, field },
                );
                return error.DeadlineEventLeaksField;
            }
        }
    }
}
