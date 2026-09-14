//! ReportSpool.zig — the runner's durable hold on a terminal report, between
//! the child exiting and the control plane acknowledging it.
//!
//! A run costs the customer a model call and a wall-clock wait. Until this
//! module, `lease_run.executeAndReport` finished the run, POSTed the terminal
//! report, and on any transport failure logged `report_failed`, slept once and
//! returned — the result went with the stack frame. The lease then expired,
//! another runner reclaimed the fleet, and the SAME event executed a second
//! time: a second model call, a second charge, and for a fleet with side
//! effects a second set of them. The work was never in doubt. Only the sentence
//! saying it finished was, and that sentence lived in one process's memory.
//!
//! So the report is written to disk BEFORE its first POST, not inside the
//! failure handler. A handler only runs for a failure the process survives; a
//! `SIGKILL`, an out-of-memory kill or a host reboot between the child exiting
//! and the acknowledgement arriving never reaches one. Spooling first is what
//! makes "this run finished" outlive the process that observed it.
//!
//! The entry holds the POST body verbatim, so a replay is a byte-for-byte
//! resend carrying the same `lease_id`, `event_id` and `fencing_token` — the
//! identity that lets the control plane recognise a retry as the same operation
//! rather than a new one. The report endpoint is idempotent on the lease id,
//! so an identical repeat returns the stored outcome and charges nothing.
//! The runner's bearer token is NOT in the body; it rides the Authorization
//! header. So no credential is persisted, and a replay authenticates with the
//! token the daemon holds at replay time instead of a stale one.
//!
//! Four boundaries the spool holds:
//!
//!   1. ATOMIC — the entry materialises through `createFileAtomic`, so a crash
//!      mid-write leaves an unnamed temporary, never a truncated report that a
//!      later drain would post as though it were whole.
//!   2. PRIVATE — owner-only permissions. A report carries the fleet's response
//!      text, which is customer content, in a home an operator chose by string.
//!   3. BOUNDED — the spool has a ceiling, and reaching it stops the daemon
//!      taking a NEW lease. It never refuses to hold a result that already
//!      exists: discarding a finished run is the outcome this module prevents,
//!      so the back-pressure lands on intake, where waiting costs nothing.
//!   4. TERMINATING — an entry the control plane will never accept moves to
//!      `quarantine/` instead of being retried forever, so one poisoned report
//!      cannot stall every later one behind it.
//!
//! Two concerns live in modules behind this one, which stays the public API:
//! `report_spool_entry.zig` (what an entry is called, which files are ours) and
//! `report_spool_replay.zig` (settling what an earlier process held).

const ReportSpool = @This();

/// The spool directory, opened once inside the claimed storage home. Every
/// operation runs relative to this handle rather than re-resolving a path, so
/// the directory cannot be swapped underneath a daemon that already holds the
/// home's exclusive lock.
dir: Dir,

/// Whether a finished result reached the disk. `unavailable` is not a soft
/// failure: the caller holds a result nothing else remembers, so it logs at
/// error and the daemon stops taking new leases until the spool works again.
pub const Held = enum { held, unavailable };

pub const Verdict = replay.Verdict;
pub const Delivery = replay.Delivery;
pub const Drained = replay.Drained;

/// Open the spool inside an already-claimed storage home, creating it on first
/// boot. Null means the daemon has no durable place to put a result; the caller
/// treats that the way it treats a full spool, because both end the same way.
///
/// The home's exclusive advisory lock is what keeps two daemons out of one
/// spool, so this deliberately takes no lock of its own — a second lock would
/// imply a second claim `StorageHome` has already refused.
pub fn open(io: Io, home: Dir) ?ReportSpool {
    home.createDir(io, SPOOL_DIR_NAME, SPOOL_DIR_PERMISSIONS) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => {
            log.err("report_spool_create_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err) });
            return null;
        },
    };
    const dir = home.openDir(io, SPOOL_DIR_NAME, .{ .iterate = true }) catch |err| {
        log.err("report_spool_open_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err) });
        return null;
    };
    return .{ .dir = dir };
}

/// Release the handle. Entries stay on disk by design — an undrained report is
/// exactly what the next boot exists to find.
pub fn close(self: *ReportSpool, io: Io) void {
    self.dir.close(io);
    // SAFETY: the descriptor is closed above, so the only field is spent.
    // Poisoning makes a use-after-close trap instead of reusing a handle number
    // the kernel may already have handed to something else (RULE A5).
    self.* = undefined;
}

/// Write `body` where a later boot will find it, keyed by `lease_id`. Call this
/// BEFORE the first POST; `release` spends the entry once the report lands.
///
/// Borrows both arguments for the call and keeps neither.
pub fn hold(self: *ReportSpool, io: Io, lease_id: []const u8, body: []const u8) Held {
    var name_buf: [entry.NAME_MAX]u8 = undefined;
    const name = entry.name(&name_buf, lease_id) orelse {
        log.err("report_spool_key_refused", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .lease_id = lease_id });
        return .unavailable;
    };
    var atomic = self.dir.createFileAtomic(io, name, .{
        .permissions = SPOOL_FILE_PERMISSIONS,
        .replace = true,
    }) catch |err| return self.writeFailed(lease_id, "report_spool_create_entry_failed", err);
    defer atomic.deinit(io);

    var write_buf: [WRITE_BUFFER_BYTES]u8 = undefined;
    var writer = atomic.file.writer(io, &write_buf);
    writer.interface.writeAll(body) catch |err|
        return self.writeFailed(lease_id, "report_spool_write_failed", err);
    writer.interface.flush() catch |err|
        return self.writeFailed(lease_id, "report_spool_flush_failed", err);
    // Only now does the name exist. Before this line a reader sees nothing;
    // after it, a whole report.
    atomic.replace(io) catch |err|
        return self.writeFailed(lease_id, "report_spool_commit_failed", err);

    log.debug("report_spool_held", .{ .lease_id = lease_id, .bytes = body.len });
    return .held;
}

/// Spend the entry for `lease_id`. Called only AFTER the control plane has
/// acknowledged the report — never before, or the acknowledgement's own loss
/// reopens the window the hold closed.
pub fn release(self: *ReportSpool, io: Io, lease_id: []const u8) void {
    var name_buf: [entry.NAME_MAX]u8 = undefined;
    const name = entry.name(&name_buf, lease_id) orelse return;
    self.dir.deleteFile(io, name) catch |err| switch (err) {
        // A drain already spent it, or it was never held. Either way the
        // caller's postcondition holds, so this is not a failure.
        error.FileNotFound => {},
        else => log.warn("report_spool_release_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .lease_id = lease_id, .err = @errorName(err) }),
    };
}

/// True when the spool is too full to accept another run's result, so the daemon
/// must stop taking NEW leases until a drain clears it. An unreadable spool
/// counts as full: a bound that cannot be measured is not a bound.
///
/// Held entries are counted; the quarantine is not, because it is an operator's
/// pile and nothing drains it on its own.
pub fn atCapacity(self: *ReportSpool, io: Io) bool {
    var entries: u32 = 0;
    var bytes: u64 = 0;
    var it = self.dir.iterate();
    while (true) {
        const next = it.next(io) catch |err| {
            log.warn("report_spool_scan_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
            return true;
        };
        const listed = next orelse break;
        if (!entry.isHeld(listed)) continue;
        entries += 1;
        if (entries >= SPOOL_MAX_ENTRIES) return true;
        const stat = self.dir.statFile(io, listed.name, .{}) catch continue;
        bytes += stat.size;
        if (bytes >= SPOOL_MAX_BYTES) return true;
    }
    return false;
}

/// Replay every held report through `delivery`. Runs at boot before the first
/// lease, so a result an earlier process could not deliver is settled before
/// this one adds to the pile. Rules and ordering live in `report_spool_replay`.
pub fn drain(self: *ReportSpool, io: Io, alloc: Allocator, delivery: Delivery) Drained {
    return replay.run(self.dir, io, alloc, delivery);
}

/// One log line and one verdict for every way the atomic write can fail. The
/// event name says which step lost it; the outcome is the same either way,
/// because a result that did not reach the disk is a result at risk.
fn writeFailed(self: *ReportSpool, lease_id: []const u8, event: []const u8, err: anyerror) Held {
    _ = self;
    log.err("report_spool_hold_failed", .{
        .error_code = ERR_EXEC_TRANSPORT_LOSS,
        .lease_id = lease_id,
        .step = event,
        .err = @errorName(err),
    });
    return .unavailable;
}

const std = @import("std");
const logging = @import("log");
const client_errors = @import("../engine/client_errors.zig");
const entry = @import("report_spool_entry.zig");
const replay = @import("report_spool_replay.zig");

const Allocator = std.mem.Allocator;
const Io = std.Io;
const Dir = std.Io.Dir;
const Permissions = std.Io.File.Permissions;
const log = logging.scoped(.fleet_runner);
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;
const ERR_EXEC_TRANSPORT_LOSS = client_errors.ERR_EXEC_TRANSPORT_LOSS;

/// Held entries live here, inside the storage home the daemon already claimed.
const SPOOL_DIR_NAME = "reports";

/// Owner-only. A report carries the fleet's response text — customer content —
/// and the storage home is an operator-supplied path that may be group-readable.
const SPOOL_FILE_PERMISSIONS: Permissions = .fromMode(0o600);
const SPOOL_DIR_PERMISSIONS: Permissions = .fromMode(0o700);

/// Stack buffer for one entry's write. A report body streams through it, so this
/// bounds the copy, not the report.
const WRITE_BUFFER_BYTES: usize = 4096;

/// How many held reports before the daemon stops taking a new lease. Sized so a
/// control plane out for a long while still parks every result it owes, while a
/// runner whose spool is genuinely stuck stops adding to the pile.
///
/// `pub` so the proof sizes itself from the bound rather than from a literal the
/// bound can drift past.
pub const SPOOL_MAX_ENTRIES: u32 = 256;
/// The same ceiling in bytes, because one fleet's response text can be large
/// where another's is a line. Whichever bound is reached first stops leasing.
const SPOOL_MAX_BYTES: u64 = 64 * 1024 * 1024;

// The two modules behind this one are force-referenced through the consts that
// already name them, so the paths appear once in the file rather than twice.
test {
    _ = entry;
    _ = replay;
    _ = @import("report_spool_test.zig");
}
