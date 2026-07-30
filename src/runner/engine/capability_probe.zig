//! What this host's kernel can actually enforce — probed at startup and
//! refreshed per heartbeat tick, reported to the control plane so the
//! reconciliation can compare assigned against achievable and mark the runner
//! degraded instead of letting it refuse work silently.
//!
//! Probes are side-effect-free: each asks availability without installing or
//! creating anything. Off-linux every mechanism reads unavailable. The report
//! assembly is pure (`assemble`) so the matrix is unit-testable anywhere; only
//! the thin collectors touch the kernel.

const std = @import("std");
const builtin = @import("builtin");
const protocol = @import("contract").protocol;
const CgroupScope = @import("CgroupScope.zig");
const sandbox_args = @import("../sandbox_args.zig");

/// Kernel-enforced egress (`EgressScope`) is not built in any current runner;
/// the report says so, and an assigned `allow_list_egress` reconciles to a
/// visible degraded row rather than implying the allowlist could be honoured.
const EGRESS_ENFORCEMENT_BUILT = false;

/// `landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)` returns
/// the ABI version without creating anything — the canonical availability ask.
const LANDLOCK_CREATE_RULESET_VERSION: usize = 1;

/// prctl op asking the current seccomp mode — answers availability without
/// installing a filter (a kernel without seccomp errors instead).
const PR_GET_SECCOMP: i32 = 21;

/// Everything the kernel answered, before assembly. Separated so `assemble`
/// stays pure and the report matrix is testable off-linux.
pub const MechanismInputs = struct {
    landlock: bool,
    seccomp: bool,
    cgroup_controllers: []const []const u8,
    bubblewrap: bool,
};

/// Pure assembly: the report mirrors each input independently and pins the
/// egress line to what this build actually contains.
pub fn assemble(inputs: MechanismInputs) protocol.CapabilityReport {
    return .{
        .landlock = inputs.landlock,
        .seccomp = inputs.seccomp,
        .cgroup_controllers = inputs.cgroup_controllers,
        .bubblewrap = inputs.bubblewrap,
        .egress_enforcement = EGRESS_ENFORCEMENT_BUILT,
    };
}

/// Probe every mechanism. The controllers slice is owned — free the report
/// with `freeReport(alloc, report)`. Never fails: an unreadable mechanism
/// reads unavailable, which is the honest degraded-side answer.
pub fn collect(io: std.Io, alloc: std.mem.Allocator) protocol.CapabilityReport {
    return assemble(.{
        .landlock = landlockAvailable(),
        .seccomp = seccompAvailable(),
        .cgroup_controllers = collectControllers(io, alloc),
        .bubblewrap = sandbox_args.bwrapPath(io) != null,
    });
}

/// Free a `collect`ed report's owned controller names. A zero-length slice is
/// the static empty fallback and owns nothing.
pub fn freeReport(alloc: std.mem.Allocator, report: protocol.CapabilityReport) void {
    if (report.cgroup_controllers.len == 0) return;
    for (report.cgroup_controllers) |c| alloc.free(c);
    alloc.free(report.cgroup_controllers);
}

/// Deep equality — the loop re-sends the report only when it changed.
pub fn eql(a: protocol.CapabilityReport, b: protocol.CapabilityReport) bool {
    if (a.landlock != b.landlock or a.seccomp != b.seccomp) return false;
    if (a.bubblewrap != b.bubblewrap or a.egress_enforcement != b.egress_enforcement) return false;
    if (a.cgroup_controllers.len != b.cgroup_controllers.len) return false;
    for (a.cgroup_controllers, b.cgroup_controllers) |x, y| {
        if (!std.mem.eql(u8, x, y)) return false;
    }
    return true;
}

fn landlockAvailable() bool {
    if (builtin.os.tag != .linux) return false;
    const rc = std.os.linux.syscall3(.landlock_create_ruleset, 0, 0, LANDLOCK_CREATE_RULESET_VERSION);
    const signed: isize = @bitCast(rc);
    return signed >= 1;
}

fn seccompAvailable() bool {
    if (builtin.os.tag != .linux) return false;
    const rc = std.os.linux.prctl(PR_GET_SECCOMP, 0, 0, 0, 0);
    return std.os.linux.E.init(rc) == .SUCCESS;
}

fn collectControllers(io: std.Io, alloc: std.mem.Allocator) []const []const u8 {
    if (builtin.os.tag != .linux) return &.{};
    const base = CgroupScope.resolveCgroupBase(io, alloc) catch return &.{};
    defer alloc.free(base);
    const path = std.fmt.allocPrint(alloc, "{s}/cgroup.subtree_control", .{base}) catch return &.{};
    defer alloc.free(path);
    const file = std.Io.Dir.openFileAbsolute(io, path, .{}) catch return &.{};
    defer file.close(io);
    var fr = file.reader(io, &.{});
    var buf: [512]u8 = undefined;
    const len = fr.interface.readSliceShort(&buf) catch return &.{};
    return parseControllers(alloc, buf[0..len]) catch &.{};
}

/// Pure: whitespace-separated controller names → owned copies. Unit-tested;
/// the kernel writes them space-separated with a trailing newline.
pub fn parseControllers(alloc: std.mem.Allocator, text: []const u8) ![]const []const u8 {
    var list: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (list.items) |x| alloc.free(x);
        list.deinit(alloc);
    }
    var it = std.mem.tokenizeAny(u8, text, " \t\r\n");
    while (it.next()) |tok| {
        const owned = try alloc.dupe(u8, tok);
        errdefer alloc.free(owned);
        try list.append(alloc, owned);
    }
    return list.toOwnedSlice(alloc);
}

test "assemble mirrors each mechanism independently and pins the egress line" {
    const none = assemble(.{ .landlock = false, .seccomp = false, .cgroup_controllers = &.{}, .bubblewrap = false });
    try std.testing.expect(!none.landlock and !none.seccomp and !none.bubblewrap);
    try std.testing.expect(!none.egress_enforcement); // not built in any current runner

    const some = assemble(.{ .landlock = true, .seccomp = false, .cgroup_controllers = &.{"cpu"}, .bubblewrap = true });
    try std.testing.expect(some.landlock);
    try std.testing.expect(!some.seccomp);
    try std.testing.expect(some.bubblewrap);
    try std.testing.expectEqual(@as(usize, 1), some.cgroup_controllers.len);
    try std.testing.expect(!some.egress_enforcement);
}

test "parseControllers splits the kernel's space-separated list, tolerating the trailing newline" {
    const a = std.testing.allocator;
    const parsed = try parseControllers(a, "cpu memory pids\n");
    defer freeReport(a, .{ .landlock = false, .seccomp = false, .cgroup_controllers = parsed, .bubblewrap = false, .egress_enforcement = false });
    try std.testing.expectEqual(@as(usize, 3), parsed.len);
    try std.testing.expectEqualStrings("cpu", parsed[0]);
    try std.testing.expectEqualStrings("pids", parsed[2]);

    const empty = try parseControllers(a, "\n");
    defer a.free(empty);
    try std.testing.expectEqual(@as(usize, 0), empty.len);
}

test "eql detects every field flip and controller drift" {
    const base = protocol.CapabilityReport{ .landlock = true, .seccomp = true, .cgroup_controllers = &.{ "cpu", "memory" }, .bubblewrap = true, .egress_enforcement = false };
    try std.testing.expect(eql(base, base));
    var flipped = base;
    flipped.landlock = false;
    try std.testing.expect(!eql(base, flipped));
    var drifted = base;
    drifted.cgroup_controllers = &.{ "cpu", "pids" };
    try std.testing.expect(!eql(base, drifted));
    var shorter = base;
    shorter.cgroup_controllers = &.{"cpu"};
    try std.testing.expect(!eql(base, shorter));
}
