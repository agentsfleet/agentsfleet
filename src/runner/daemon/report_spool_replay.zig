//! report_spool_replay.zig — settling the reports an earlier process held but
//! could not deliver.
//!
//! Extracted from `ReportSpool.zig` as the replay concern (Module Split
//! Pattern). Intake runs once per lease and is cheap; replay runs at boot,
//! reads bodies back, and talks to the control plane. Different lifecycles,
//! different failure modes, so they are different modules — `ReportSpool`
//! remains the public API and this one is an implementation detail behind it.
//!
//! The POST is injected as `Delivery` rather than imported, so every rule below
//! is testable with no control plane and no network. That is the same `ctx` +
//! function-pointer shape `child_supervisor`'s renew and mint hooks use.
//!
//! Ordering is load-bearing: an entry is removed only AFTER the control plane
//! acknowledges it. A delete-then-post would reopen the window the spool exists
//! to close — the process could die between the two and the result would be
//! gone with the file that proved it.

/// What the control plane said about one replayed report. Every variant is a
/// decision `run` acts on, so a caller cannot strand an entry by returning a
/// value the replay has no rule for.
pub const Verdict = enum {
    /// Accepted, or recognised as an already-settled repeat. The entry is spent.
    acknowledged,
    /// A transport or server fault. The entry stays for the next drain.
    retry_later,
    /// The lease was reclaimed and re-leased, so fencing rejected this report
    /// and a replacement now owns the event. The entry is spent — dropping it
    /// rather than forcing it is what "never overwrite a replacement" means.
    superseded,
    /// Malformed or permanently rejected. No retry can change the answer, so the
    /// entry is quarantined for an operator instead of spinning forever.
    invalid,
};

/// The POST, as a dependency rather than an import.
pub const Delivery = struct {
    ctx: *anyopaque,
    postFn: *const fn (ctx: *anyopaque, body: []const u8) Verdict,

    fn post(self: Delivery, body: []const u8) Verdict {
        return self.postFn(self.ctx, body);
    }
};

/// What one drain did. Separate counters rather than a total, so a caller (and
/// a test) reads which rule fired instead of inferring it from a sum.
pub const Drained = struct {
    acknowledged: u32 = 0,
    kept: u32 = 0,
    superseded: u32 = 0,
    quarantined: u32 = 0,
};

/// Replay every held report in `dir` through `delivery`, spending, keeping or
/// quarantining each by its verdict.
///
/// Names are collected before any delivery, because removing an entry
/// mid-iteration leaves the readdir cursor free to skip entries it has not yet
/// returned — which would silently strand reports on exactly the boot that
/// exists to deliver them. The same reason `StorageHome.sweep` batches.
///
/// Borrows `dir` and `delivery`; every allocation it makes is freed before it
/// returns.
pub fn run(dir: Dir, io: Io, alloc: Allocator, delivery: Delivery) Drained {
    var drained: Drained = .{};
    var batch: [DRAIN_BATCH][entry.NAME_MAX]u8 = undefined;
    var lengths: [DRAIN_BATCH]usize = undefined;
    const found = collectHeld(dir, io, &batch, &lengths);

    for (batch[0..found], lengths[0..found]) |*name_buf, len| {
        const name = name_buf[0..len];
        const body = dir.readFileAlloc(io, name, alloc, .limited(ENTRY_MAX_BYTES)) catch |err| {
            // A body no POST could carry is a body no retry can fix, so it
            // leaves the replay set instead of blocking everything behind it.
            log.warn("report_spool_entry_unreadable", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .entry = name, .err = @errorName(err) });
            quarantine(dir, io, name);
            drained.quarantined += 1;
            continue;
        };
        defer alloc.free(body);

        switch (delivery.post(body)) {
            .acknowledged => {
                deleteHeld(dir, io, name);
                drained.acknowledged += 1;
                log.info("report_spool_delivered", .{ .entry = name });
            },
            .retry_later => {
                drained.kept += 1;
                log.info("report_spool_kept", .{ .entry = name });
            },
            .superseded => {
                deleteHeld(dir, io, name);
                drained.superseded += 1;
                log.info("report_spool_superseded", .{ .entry = name });
            },
            .invalid => {
                quarantine(dir, io, name);
                drained.quarantined += 1;
                log.warn("report_spool_quarantined", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .entry = name });
            },
        }
    }
    return drained;
}

/// Move an entry out of the replay set and into an operator's pile. A failed
/// move KEEPS the entry held rather than deleting it: an entry nobody can read
/// is still evidence a run finished, and losing it quietly is the one outcome
/// this module refuses.
fn quarantine(dir: Dir, io: Io, name: []const u8) void {
    dir.createDir(io, entry.QUARANTINE_DIR_NAME, QUARANTINE_PERMISSIONS) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => {
            log.err("report_spool_quarantine_unavailable", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .entry = name, .err = @errorName(err) });
            return;
        },
    };
    var dest_buf: [entry.QUARANTINE_PATH_MAX]u8 = undefined;
    const dest = entry.quarantinePath(&dest_buf, name) orelse return;
    dir.rename(name, dir, dest, io) catch |err| {
        log.err("report_spool_quarantine_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .entry = name, .err = @errorName(err) });
    };
}

fn deleteHeld(dir: Dir, io: Io, name: []const u8) void {
    dir.deleteFile(io, name) catch |err| {
        log.warn("report_spool_delete_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .entry = name, .err = @errorName(err) });
    };
}

/// Fill `batch` with the names of up to `DRAIN_BATCH` held entries, writing each
/// length into `lengths`. Returns how many were written. A scan error ends the
/// pass with what it has — the next boot sees the rest.
fn collectHeld(
    dir: Dir,
    io: Io,
    batch: *[DRAIN_BATCH][entry.NAME_MAX]u8,
    lengths: *[DRAIN_BATCH]usize,
) usize {
    var found: usize = 0;
    var it = dir.iterate();
    while (found < batch.len) {
        const next = it.next(io) catch |err| {
            log.warn("report_spool_scan_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
            return found;
        };
        const listed = next orelse return found;
        if (!entry.isHeld(listed)) continue;
        @memcpy(batch[found][0..listed.name.len], listed.name);
        lengths[found] = listed.name.len;
        found += 1;
    }
    return found;
}

const std = @import("std");
const logging = @import("log");
const client_errors = @import("../engine/client_errors.zig");
const entry = @import("report_spool_entry.zig");

const Allocator = std.mem.Allocator;
const Io = std.Io;
const Dir = std.Io.Dir;
const log = logging.scoped(.fleet_runner);
const ERR_EXEC_TRANSPORT_LOSS = client_errors.ERR_EXEC_TRANSPORT_LOSS;

/// Owner-only, like the entries it holds: a quarantined report still carries the
/// fleet's response text.
const QUARANTINE_PERMISSIONS: std.Io.File.Permissions = .fromMode(0o700);

/// The largest single entry a drain reads back. A body past this is one no POST
/// would carry, so it is quarantined rather than retried.
const ENTRY_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Entries replayed per drain. A boot holding more settles the rest on the next
/// pass rather than holding the first lease behind an unbounded loop.
const DRAIN_BATCH: usize = 64;

/// When the next drain is due, as a countdown of control-loop ticks that grows
/// while nothing settles.
///
/// A held report the control plane is still refusing must not be re-POSTed on
/// every heartbeat: that turns one outage into a request storm precisely when
/// the far side is least able to absorb it. So the interval doubles after a pass
/// that settled nothing it was holding, and snaps back the moment one settles.
///
/// Ticks rather than milliseconds, because the control loop already has a
/// cadence and a second clock would only disagree with it.
pub const Cadence = struct {
    remaining: u32 = 0,
    interval: u32 = MIN_INTERVAL_TICKS,

    /// The floor: often enough that a report a worker held moments ago is
    /// delivered while the daemon is still up, rare enough to be free.
    pub const MIN_INTERVAL_TICKS: u32 = 4;
    /// The ceiling, so a long outage settles into an occasional knock.
    pub const MAX_INTERVAL_TICKS: u32 = 256;

    /// True when this tick should drain; advances the countdown either way.
    pub fn due(self: *Cadence) bool {
        if (self.remaining > 0) {
            self.remaining -= 1;
            return false;
        }
        self.remaining = self.interval;
        return true;
    }

    /// Fold what a drain did back into the interval. Anything settled — even a
    /// quarantine — is progress and resets it; a pass that only KEPT entries is
    /// the control plane still refusing, and backs further off. A pass that
    /// found nothing stays at the floor, so a newly held report is not made to
    /// wait out a backoff earned by an outage that has since ended.
    pub fn observe(self: *Cadence, drained: Drained) void {
        const settled = drained.acknowledged + drained.superseded + drained.quarantined;
        if (settled > 0 or drained.kept == 0) {
            self.interval = MIN_INTERVAL_TICKS;
            return;
        }
        self.interval = @min(self.interval * 2, MAX_INTERVAL_TICKS);
    }
};

test "Cadence fires on the first tick, then only when due" {
    var cadence: Cadence = .{};
    try std.testing.expect(cadence.due()); // the boot tick drains
    var ticks: u32 = 0;
    while (ticks < Cadence.MIN_INTERVAL_TICKS) : (ticks += 1) {
        try std.testing.expect(!cadence.due());
    }
    try std.testing.expect(cadence.due());
}

test "Cadence backs off only while the control plane keeps refusing" {
    var cadence: Cadence = .{};
    // Nothing settled and something kept: the far side is still refusing.
    cadence.observe(.{ .kept = 1 });
    try std.testing.expectEqual(Cadence.MIN_INTERVAL_TICKS * 2, cadence.interval);
    cadence.observe(.{ .kept = 1 });
    try std.testing.expectEqual(Cadence.MIN_INTERVAL_TICKS * 4, cadence.interval);
    // One delivery means the outage is over; the next held report is not made
    // to wait out a backoff the previous one earned.
    cadence.observe(.{ .acknowledged = 1, .kept = 1 });
    try std.testing.expectEqual(Cadence.MIN_INTERVAL_TICKS, cadence.interval);
    // A quarantine is progress too — the pile shrank, the loop is not stuck.
    cadence.observe(.{ .kept = 1 });
    cadence.observe(.{ .quarantined = 1, .kept = 1 });
    try std.testing.expectEqual(Cadence.MIN_INTERVAL_TICKS, cadence.interval);
    // An empty spool stays at the floor rather than drifting out.
    cadence.observe(.{});
    try std.testing.expectEqual(Cadence.MIN_INTERVAL_TICKS, cadence.interval);
}

test "Cadence backoff stops at its ceiling" {
    var cadence: Cadence = .{};
    var passes: u32 = 0;
    while (passes < 64) : (passes += 1) cadence.observe(.{ .kept = 1 });
    try std.testing.expectEqual(Cadence.MAX_INTERVAL_TICKS, cadence.interval);
}
