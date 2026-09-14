//! Dimension 7.2 — a runner that finishes an event keeps the result until the
//! control plane says it has it.
//!
//! Every test owns a real temp directory. The spool's whole job is to survive a
//! process that no longer exists, so an in-memory fake would prove nothing about
//! the thing that can actually go wrong: the file has to be on a disk, whole,
//! private, and findable by a process that did not write it.
//!
//! The POST is the injected `Delivery`, so a "restart" here is honest — the
//! second `ReportSpool.open` is a different handle over the same directory, the
//! way a rebooted daemon gets one, and the bytes it replays are the bytes the
//! first one wrote, not a value carried in memory between the two.
//!
//! The naming and classification rules are pure and are pinned inline in
//! `report_spool_entry.zig`, next to the prose they encode.

const std = @import("std");
const ReportSpool = @import("ReportSpool.zig");
const loop_spool = @import("loop_spool.zig");

const io = @import("common").globalIo();
const Dir = std.Io.Dir;

/// A well-formed lease id — the shape `fleet/service.zig` mints.
const LEASE_A = "0199a4c1-8f3e-7b21-9c4d-2f6a1e8b7d05";
const LEASE_B = "0199a4c2-1b77-7f40-8e13-5a9c0d2e4f61";

/// A terminal report body, as `control_plane_client.report` would stringify it.
/// The three identity fields are what a replay has to carry unchanged.
const BODY_A =
    \\{"lease_id":"0199a4c1-8f3e-7b21-9c4d-2f6a1e8b7d05","event_id":"0199b000-0000-7000-8000-000000000001","fencing_token":42,"outcome":"processed","response_text":"done"}
;
const BODY_B =
    \\{"lease_id":"0199a4c2-1b77-7f40-8e13-5a9c0d2e4f61","event_id":"0199b000-0000-7000-8000-000000000002","fencing_token":43,"outcome":"processed","response_text":"also done"}
;

/// Wide enough for the 36-byte dashed UUID text the capacity proof renders.
const ID_BUF: usize = 48;

/// Owner-only: a report carries the fleet's response text.
const EXPECTED_FILE_MODE: std.posix.mode_t = 0o600;

/// A scripted control plane. Records every body it was handed, so a test asserts
/// the replay carried the same bytes rather than trusting that it did.
const FakePlane = struct {
    verdict: ReportSpool.Verdict,
    calls: u32 = 0,
    last: [BODY_MAX]u8 = undefined,
    last_len: usize = 0,

    const BODY_MAX: usize = 512;

    fn post(ctx: *anyopaque, body: []const u8) ReportSpool.Verdict {
        const self: *FakePlane = @ptrCast(@alignCast(ctx));
        self.calls += 1;
        @memcpy(self.last[0..body.len], body);
        self.last_len = body.len;
        return self.verdict;
    }

    fn delivery(self: *FakePlane) ReportSpool.Delivery {
        return .{ .ctx = self, .postFn = &post };
    }

    fn seen(self: *const FakePlane) []const u8 {
        return self.last[0..self.last_len];
    }
};

/// Fresh absolute temp home for one test; a stale tree from a previous run is
/// deleted first and the tree is deleted again on exit.
fn freshHome(comptime name: []const u8) ![]const u8 {
    const path = "/tmp/agentsfleet-spool-test-" ++ name;
    try Dir.cwd().deleteTree(io, path); // idempotent on a missing path
    try Dir.createDirAbsolute(io, path, .default_dir);
    return path;
}

/// Open the spool the way a booting daemon does: over the home directory, with
/// no memory of any earlier handle.
fn openOver(home: []const u8) !ReportSpool {
    var dir = try Dir.openDirAbsolute(io, home, .{ .iterate = true });
    defer dir.close(io);
    return ReportSpool.open(io, dir) orelse error.SpoolUnavailable;
}

fn heldCount(home: []const u8) !u32 {
    var spool = try openOver(home);
    defer spool.close(io);
    var count: u32 = 0;
    var it = spool.dir.iterate();
    while (try it.next(io)) |entry| {
        if (entry.kind == .file and std.mem.endsWith(u8, entry.name, ".report.json")) count += 1;
    }
    return count;
}

test "test_runner_retains_terminal_report_until_acknowledged" {
    const home = try freshHome("retains");
    defer Dir.cwd().deleteTree(io, home) catch {};

    // ── The run finishes and the result is spooled BEFORE its first POST.
    {
        var spool = try openOver(home);
        defer spool.close(io);
        try std.testing.expectEqual(ReportSpool.Held.held, spool.hold(io, LEASE_A, BODY_A));
    }

    // ── That POST fails, so nothing is released. The process then dies: no
    //    handler runs, no memory survives, only the file.
    try std.testing.expectEqual(@as(u32, 1), try heldCount(home));

    // ── A new process boots and drains before taking a lease. The control plane
    //    is still down, so the entry is kept, not lost.
    {
        var spool = try openOver(home);
        defer spool.close(io);
        var plane = FakePlane{ .verdict = .retry_later };
        const drained = spool.drain(io, std.testing.allocator, plane.delivery());
        try std.testing.expectEqual(@as(u32, 1), drained.kept);
        try std.testing.expectEqual(@as(u32, 0), drained.acknowledged);
        // The identity a retry has to carry: the same bytes, so the same
        // lease_id, event_id and fencing_token the first attempt sent.
        try std.testing.expectEqualStrings(BODY_A, plane.seen());
    }
    try std.testing.expectEqual(@as(u32, 1), try heldCount(home));

    // ── The control plane comes back. Now, and only now, the entry is spent.
    {
        var spool = try openOver(home);
        defer spool.close(io);
        var plane = FakePlane{ .verdict = .acknowledged };
        const drained = spool.drain(io, std.testing.allocator, plane.delivery());
        try std.testing.expectEqual(@as(u32, 1), drained.acknowledged);
        try std.testing.expectEqualStrings(BODY_A, plane.seen());
    }
    try std.testing.expectEqual(@as(u32, 0), try heldCount(home));
}

test "a held report is written whole and owner-only" {
    const home = try freshHome("private");
    defer Dir.cwd().deleteTree(io, home) catch {};

    var spool = try openOver(home);
    defer spool.close(io);
    try std.testing.expectEqual(ReportSpool.Held.held, spool.hold(io, LEASE_A, BODY_A));

    const name = LEASE_A ++ ".report.json";
    const stat = try spool.dir.statFile(io, name, .{});
    // A report is customer content in an operator-chosen directory, so group
    // and world get nothing.
    try std.testing.expectEqual(EXPECTED_FILE_MODE, stat.permissions.toMode() & 0o777);

    const body = try spool.dir.readFileAlloc(io, name, std.testing.allocator, .limited(4096));
    defer std.testing.allocator.free(body);
    try std.testing.expectEqualStrings(BODY_A, body);
}

test "an acknowledged report releases without waiting for a drain" {
    const home = try freshHome("release");
    defer Dir.cwd().deleteTree(io, home) catch {};

    var spool = try openOver(home);
    defer spool.close(io);
    _ = spool.hold(io, LEASE_A, BODY_A);
    spool.release(io, LEASE_A);
    try std.testing.expectEqual(@as(u32, 0), try heldCount(home));

    // Releasing what is not held is the caller's postcondition either way.
    spool.release(io, LEASE_B);
}

test "a superseded report is dropped, never forced over its replacement" {
    const home = try freshHome("superseded");
    defer Dir.cwd().deleteTree(io, home) catch {};

    var spool = try openOver(home);
    defer spool.close(io);
    _ = spool.hold(io, LEASE_A, BODY_A);

    var plane = FakePlane{ .verdict = .superseded };
    const drained = spool.drain(io, std.testing.allocator, plane.delivery());
    try std.testing.expectEqual(@as(u32, 1), drained.superseded);
    try std.testing.expectEqual(@as(u32, 0), drained.acknowledged);
    // Gone from the replay set: fencing already gave the event to someone else,
    // so retrying this body forever would only argue with the winner.
    try std.testing.expectEqual(@as(u32, 0), try heldCount(home));
}

test "a permanently invalid report is quarantined instead of retried forever" {
    const home = try freshHome("quarantine");
    defer Dir.cwd().deleteTree(io, home) catch {};

    var spool = try openOver(home);
    defer spool.close(io);
    _ = spool.hold(io, LEASE_A, BODY_A);
    _ = spool.hold(io, LEASE_B, BODY_B);

    var plane = FakePlane{ .verdict = .invalid };
    const drained = spool.drain(io, std.testing.allocator, plane.delivery());
    try std.testing.expectEqual(@as(u32, 2), drained.quarantined);
    // Out of the replay set, so neither stalls the entries behind it...
    try std.testing.expectEqual(@as(u32, 0), try heldCount(home));
    // ...but still on disk for an operator: quarantined is not discarded.
    var pile = try spool.dir.openDir(io, "quarantine", .{ .iterate = true });
    defer pile.close(io);
    var kept: u32 = 0;
    var it = pile.iterate();
    while (try it.next(io)) |entry| {
        if (entry.kind == .file) kept += 1;
    }
    try std.testing.expectEqual(@as(u32, 2), kept);
}

test "a drain ignores files that are not ours" {
    const home = try freshHome("strays");
    defer Dir.cwd().deleteTree(io, home) catch {};

    var spool = try openOver(home);
    defer spool.close(io);
    _ = spool.hold(io, LEASE_A, BODY_A);
    try spool.dir.writeFile(io, .{ .sub_path = "operator-notes.txt", .data = "do not post me" });

    var plane = FakePlane{ .verdict = .acknowledged };
    const drained = spool.drain(io, std.testing.allocator, plane.delivery());
    try std.testing.expectEqual(@as(u32, 1), drained.acknowledged);
    try std.testing.expectEqual(@as(u32, 1), plane.calls);
    try std.testing.expectEqualStrings(BODY_A, plane.seen());
}

test "a lease id that could leave the spool is refused rather than written" {
    const home = try freshHome("traversal");
    defer Dir.cwd().deleteTree(io, home) catch {};

    var spool = try openOver(home);
    defer spool.close(io);
    try std.testing.expectEqual(ReportSpool.Held.unavailable, spool.hold(io, "../escaped", BODY_A));
    try std.testing.expectEqual(@as(u32, 0), try heldCount(home));
}

test "an empty spool drains to nothing and never calls the control plane" {
    const home = try freshHome("empty");
    defer Dir.cwd().deleteTree(io, home) catch {};

    var spool = try openOver(home);
    defer spool.close(io);
    var plane = FakePlane{ .verdict = .acknowledged };
    const drained = spool.drain(io, std.testing.allocator, plane.delivery());
    try std.testing.expectEqual(@as(u32, 0), drained.acknowledged);
    try std.testing.expectEqual(@as(u32, 0), plane.calls);
    try std.testing.expect(!spool.atCapacity(io));
}

test "a full spool reports capacity so the daemon stops taking new leases" {
    const home = try freshHome("capacity");
    defer Dir.cwd().deleteTree(io, home) catch {};

    var spool = try openOver(home);
    defer spool.close(io);
    try std.testing.expect(!spool.atCapacity(io));

    // Sized from the bound itself, never a literal it could drift past: one
    // short of the ceiling still leases, the ceiling does not.
    var held: u32 = 0;
    while (held < ReportSpool.SPOOL_MAX_ENTRIES - 1) : (held += 1) {
        var id_buf: [ID_BUF]u8 = undefined;
        const id = try std.fmt.bufPrint(&id_buf, "0199a4c1-0000-7000-8000-{d:0>12}", .{held});
        try std.testing.expectEqual(ReportSpool.Held.held, spool.hold(io, id, BODY_A));
    }
    try std.testing.expect(!spool.atCapacity(io));

    var last_buf: [ID_BUF]u8 = undefined;
    const last = try std.fmt.bufPrint(&last_buf, "0199a4c1-0000-7000-8000-{d:0>12}", .{held});
    // The result that reaches the ceiling is still HELD — the back-pressure
    // lands on the next lease, never on a run that already finished.
    try std.testing.expectEqual(ReportSpool.Held.held, spool.hold(io, last, BODY_A));
    try std.testing.expect(spool.atCapacity(io));
}

test "the poll gate refuses a lease exactly when the spool is full" {
    const home = try freshHome("gate");
    defer Dir.cwd().deleteTree(io, home) catch {};

    var spool = try openOver(home);
    defer spool.close(io);
    // No spool at all never blocks a lease: a daemon with no storage home
    // reports the in-memory way it always did.
    try std.testing.expect(!loop_spool.refuseWhenFull(io, null, 0, 0));
    try std.testing.expect(!loop_spool.refuseWhenFull(io, &spool, 0, 0));

    var held: u32 = 0;
    while (held < ReportSpool.SPOOL_MAX_ENTRIES) : (held += 1) {
        var id_buf: [ID_BUF]u8 = undefined;
        const id = try std.fmt.bufPrint(&id_buf, "0199a4c1-0000-7000-8000-{d:0>12}", .{held});
        _ = spool.hold(io, id, BODY_A);
    }
    try std.testing.expect(loop_spool.refuseWhenFull(io, &spool, 0, 0));
}
