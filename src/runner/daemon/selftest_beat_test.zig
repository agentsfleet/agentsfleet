//! Unit tier for the verdict a runner carries between beats.
//!
//! `capture` spawns a real sandbox and belongs to the integration lane. The
//! hand-off shape it feeds — what rides the beat, and when nothing does — is
//! pure and proven here.

const std = @import("std");
const contract = @import("contract");
const protocol = contract.protocol;

const selftest = @import("../selftest.zig");
const selftest_beat = @import("selftest_beat.zig");

fn checks(alloc: std.mem.Allocator, all_pass: bool) ![]selftest.Check {
    const c = try alloc.alloc(selftest.Check, 2);
    c[0] = .{ .name = selftest.CHECK_RESOLVER, .ok = true, .detail = selftest.DETAIL_OK };
    c[1] = .{ .name = selftest.CHECK_DNS, .ok = all_pass, .detail = selftest.DETAIL_OK };
    return c;
}

test "a runner with no verdict attaches nothing to its beat" {
    // The heartbeat must stay a heartbeat. An empty report would overwrite the
    // stored verdict on every tick and wipe the last real one.
    var pending = selftest_beat.Pending.init(std.testing.allocator);
    defer pending.deinit();
    try std.testing.expectEqual(@as(?protocol.SelftestReport, null), pending.report());
}

test "the held verdict rides the beat carrying the policy it ran under" {
    // The policy travels WITH the result so the page can label a verdict stale
    // rather than present it against an assignment nothing tested (Invariant 4).
    const alloc = std.testing.allocator;
    var pending = selftest_beat.Pending.init(alloc);
    defer pending.deinit();
    pending.result = .{
        .checks = try checks(alloc, true),
        .network_policy = .deny_all_egress,
        .sandbox_tier = .landlock_full,
    };

    const r = pending.report().?;
    try std.testing.expectEqualStrings("deny_all_egress", r.network_policy);
    try std.testing.expectEqualStrings("landlock_full", r.sandbox_tier);
    try std.testing.expect(r.all_ok);
    try std.testing.expectEqual(@as(usize, 2), r.checks.len);
}

test "all_ok is derived from the checks, never asserted alongside them" {
    // The control plane refuses a report whose `all_ok` contradicts its checks
    // (`selftestReportRejection`). Deriving it here is what makes that
    // impossible rather than merely unlikely.
    const alloc = std.testing.allocator;
    var pending = selftest_beat.Pending.init(alloc);
    defer pending.deinit();
    pending.result = .{
        .checks = try checks(alloc, false),
        .network_policy = .allow_all,
        .sandbox_tier = .landlock_full,
    };

    const r = pending.report().?;
    try std.testing.expect(!r.all_ok);
    try std.testing.expectEqual(.none, protocol.selftestReportRejection(r));
}

test "a verdict the control plane took is not sent twice" {
    // `clear` runs on every accepted beat. Re-sending would re-stamp the
    // completion time and make a stale verdict read as a fresh one.
    const alloc = std.testing.allocator;
    var pending = selftest_beat.Pending.init(alloc);
    defer pending.deinit();
    pending.result = .{
        .checks = try checks(alloc, true),
        .network_policy = .allow_all,
        .sandbox_tier = .landlock_full,
    };
    try std.testing.expect(pending.report() != null);

    pending.clear();
    try std.testing.expectEqual(@as(?protocol.SelftestReport, null), pending.report());
    // Idempotent: the loop clears on every beat, verdict or not.
    pending.clear();
    try std.testing.expectEqual(@as(?protocol.SelftestReport, null), pending.report());
}

test "the probe workspace is the runner's own, never a lease's" {
    // A probe writing into tenant scratch would be a cross-lease surface. The
    // name is pinned so a rename cannot quietly point it at a shared path.
    try std.testing.expectEqualStrings(".selftest", selftest_beat.WORKSPACE_DIR_NAME);
}

test "the control loop probes on an ask, once at startup, and never without an assignment" {
    // The loop's own decision, lifted out so it is provable without a
    // heartbeat. The startup arm is Dimension 2.6: a freshly deployed broken
    // runner must report itself before anyone clicks.
    const S = selftest_beat.shouldCapture;

    // Startup: probe once, then stop probing unbidden.
    try std.testing.expect(S(false, false, true));
    try std.testing.expect(!S(false, true, true));
    // An operator ask always probes, however many have run before.
    try std.testing.expect(S(true, true, true));
    // No assignment, no probe — a verdict under the bootstrap config would be
    // graded against a policy the runner page is not showing.
    try std.testing.expect(!S(true, false, false));
    try std.testing.expect(!S(false, false, false));
}
