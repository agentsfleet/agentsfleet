//! StorageHome.zig — the runner's exclusive claim on `RUNNER_STORAGE_HOME`, and
//! the startup sweep of the per-lease workspaces an unclean shutdown orphaned.
//!
//! Per-lease cleanup is `defer cleanupWorkspace` (`lease_run.zig`), which does
//! not run on `SIGKILL`, an out-of-memory kill, a panic, or a host reboot; boot
//! only `mkdir`s the home and never looks inside it. That leaked at most a few
//! hundred KiB of bundle support files and nobody noticed. Once a workspace can
//! hold a repository working tree, the same shutdown orphans it permanently with
//! no collector.
//!
//! At startup no lease is held, so every per-lease workspace under the home is
//! orphaned BY DEFINITION — which is what makes the sweep trivially correct.
//! What is not trivially correct is proving the swept directory is ours: the
//! home is an operator-supplied string with no canonicalization, no marker, and
//! no lock, so a bare "delete every non-dot entry" lets a stray value or a
//! second daemon mid-rolling-deploy reap host data or live work. A claim is
//! therefore four proofs, and all four hold before one entry is removed:
//!
//!   1. CANONICAL — resolved through the open handle, so a symlinked home is
//!      swept where it actually lives, and a path too shallow to be a storage
//!      home (`/`, `/tmp`, `/home`) is refused outright.
//!   2. LOCK — an exclusive advisory lock held for the PROCESS lifetime, not for
//!      the sweep: a second daemon sharing the home cannot take it, so it skips
//!      its sweep rather than reaping the outgoing daemon's live leases.
//!   3. SENTINEL — the home carries this daemon's marker. Creating it is the
//!      adoption boot and reaps NOTHING: a fresh home has no orphans, so
//!      declining to reap a directory we have never owned costs one restart in
//!      the upgrade case and forecloses the stray-value case entirely.
//!   4. NAME + KIND — only a real directory named like a lease id is removed.
//!      Dot-prefixed entries (the bundle cache) are kept, exactly as the
//!      per-lease cleanup keeps them, and a symlink is never followed.
//!
//! Ordering is load-bearing: the lock is taken BEFORE the sentinel is tested, so
//! two daemons racing a fresh home cannot both adopt it.

const StorageHome = @This();

/// The claimed home, opened once. Every sweep operation runs relative to this
/// handle rather than re-resolving the operator's string, so the directory
/// cannot be swapped underneath the daemon after the claim.
dir: Dir,
/// Exclusive advisory lock, held until `close`. Releasing it early would let a
/// second daemon sweep this home while our leases are live.
lock: File,

/// What one startup claim-and-sweep did. `reaped` is the only outcome that
/// removed anything; every other variant is a named reason it did not, so a
/// caller (and a test) reads the refusal instead of inferring it from a zero.
pub const Outcome = union(enum) {
    /// Claimed, sentinel present, this many orphaned workspaces removed.
    reaped: u32,
    /// Claimed and the sentinel was written this boot — nothing reaped, by design.
    adopted,
    /// Another process holds the home. The daemon runs on; it does not reap.
    contended,
    /// The canonical path is too shallow to be a storage home (a stray value).
    refused_shallow,
    /// The home could not be opened, canonicalized, or locked at all.
    unavailable,
};

/// The claim to hold for the process lifetime, plus what the sweep did. `home`
/// is non-null only when this daemon holds the lock — the caller closes it at
/// shutdown; every other path has already released what it opened.
pub const Startup = struct {
    home: ?StorageHome,
    outcome: Outcome,
};

/// Claim the storage home and sweep the workspaces an unclean shutdown left
/// behind. Runs once at boot, after the home's `mkdir` and before the poll loop,
/// where "no lease is held" is a fact rather than a hope. Never fails the daemon:
/// an unclaimable home is logged and the runner continues without reaping.
pub fn claimAndSweep(io: Io, path: []const u8) Startup {
    var dir = Dir.openDirAbsolute(io, path, .{ .iterate = true }) catch |err| {
        log.warn("storage_home_open_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .path = path, .err = @errorName(err) });
        return .{ .home = null, .outcome = .unavailable };
    };

    var canonical_buf: [std.fs.max_path_bytes]u8 = undefined;
    const canonical_len = dir.realPath(io, &canonical_buf) catch |err| {
        log.warn("storage_home_canonicalize_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .path = path, .err = @errorName(err) });
        dir.close(io);
        return .{ .home = null, .outcome = .unavailable };
    };
    const canonical = canonical_buf[0..canonical_len];
    if (!isSweepablePath(canonical)) {
        log.warn("storage_home_refused_shallow", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .path = canonical });
        dir.close(io);
        return .{ .home = null, .outcome = .refused_shallow };
    }

    // Before the sentinel, so two daemons racing a fresh home cannot both adopt.
    var lock = dir.createFile(io, LOCK_NAME, .{ .truncate = false }) catch |err| {
        log.warn("storage_home_lock_open_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .path = canonical, .err = @errorName(err) });
        dir.close(io);
        return .{ .home = null, .outcome = .unavailable };
    };
    const held = lock.tryLock(io, .exclusive) catch |err| {
        log.warn("storage_home_lock_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .path = canonical, .err = @errorName(err) });
        lock.close(io);
        dir.close(io);
        return .{ .home = null, .outcome = .unavailable };
    };
    if (!held) {
        log.warn("storage_home_contended", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .path = canonical });
        lock.close(io);
        dir.close(io);
        return .{ .home = null, .outcome = .contended };
    }

    var self = StorageHome{ .dir = dir, .lock = lock };
    if (!self.sentinelExists(io)) {
        log.info("storage_home_adopted", .{ .path = canonical });
        return .{ .home = self, .outcome = .adopted };
    }

    const reaped = self.sweep(io);
    log.info("storage_home_claimed", .{ .path = canonical, .reaped = reaped });
    return .{ .home = self, .outcome = .{ .reaped = reaped } };
}

/// Release the claim. The lock is released by closing its file (and by process
/// exit, however abrupt) — the daemon holds both for its whole life.
pub fn close(self: *StorageHome, io: Io) void {
    self.lock.close(io);
    self.dir.close(io);
    // SAFETY: both descriptors are closed above, so every field is spent. Poisoning
    // makes a use-after-close trap instead of reusing a stale handle number that the
    // kernel may already have handed to something else (RULE A5).
    self.* = undefined;
}

/// True when the marker is already present. Creating it exclusively is the test:
/// success means this boot adopted a home we had never owned, so the caller
/// reaps nothing; `PathAlreadyExists` means a previous boot claimed it. Any other
/// error is treated as "not ours" — a home we cannot mark is one we do not reap.
fn sentinelExists(self: *StorageHome, io: Io) bool {
    const created = self.dir.createFile(io, SENTINEL_NAME, .{ .exclusive = true }) catch |err| switch (err) {
        error.PathAlreadyExists => return true,
        else => {
            log.warn("storage_home_sentinel_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err) });
            return false;
        },
    };
    created.close(io);
    return false;
}

/// Remove every orphaned per-lease workspace, returning how many went. Entries
/// are collected a batch at a time and deleted with the iterator closed, because
/// removing entries mid-iteration leaves the readdir cursor free to skip the
/// ones it has not yet returned — which would silently leave orphans behind on
/// exactly the boot that exists to remove them. Each pass deletes at least one
/// entry when it found any, so the pass loop terminates.
fn sweep(self: *StorageHome, io: Io) u32 {
    var reaped: u32 = 0;
    var pass: u32 = 0;
    while (pass < MAX_SWEEP_PASSES) : (pass += 1) {
        var batch: [SWEEP_BATCH][UUID_TEXT_LEN]u8 = undefined;
        const found = self.collectOrphans(io, &batch);
        if (found == 0) return reaped;
        for (batch[0..found]) |*entry| {
            const name: []const u8 = entry;
            self.dir.deleteTree(io, name) catch |err| {
                log.warn("orphan_workspace_reap_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .lease_id = name, .err = @errorName(err) });
                continue;
            };
            reaped += 1;
            log.info("orphan_workspace_reaped", .{ .lease_id = name });
        }
        if (found < SWEEP_BATCH) return reaped;
    }
    log.warn("storage_sweep_truncated", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .reaped = reaped, .passes = pass });
    return reaped;
}

/// Fill `batch` with the names of up to `SWEEP_BATCH` reapable entries, scanning
/// from the top of the directory. Returns how many were written. A scan error
/// ends the pass with what it has — the next boot sees the rest.
fn collectOrphans(self: *StorageHome, io: Io, batch: *[SWEEP_BATCH][UUID_TEXT_LEN]u8) usize {
    var found: usize = 0;
    var scanned: u32 = 0;
    var it = self.dir.iterate();
    while (found < batch.len and scanned < MAX_SCANNED_ENTRIES) {
        const next = it.next(io) catch |err| {
            log.warn("storage_home_scan_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err) });
            return found;
        };
        const entry = next orelse return found;
        scanned += 1;
        // A symlink is `.sym_link` here, never `.directory`, so a link named
        // like a lease id is skipped rather than followed out of the home.
        if (entry.kind != .directory) continue;
        if (!isLeaseWorkspaceName(entry.name)) continue;
        batch[found] = entry.name[0..UUID_TEXT_LEN].*;
        found += 1;
    }
    return found;
}

/// True when `name` is the canonical dashed UUID text a lease id is rendered as
/// (`fleet/service.zig` mints one per lease; `lease_run.prepareWorkspace` names
/// the workspace after it). This is a SHAPE check on a directory name, not an id
/// validator — `types/id_format.zig` owns that and lives in the control plane's
/// module graph, which the runner binary deliberately cannot reach.
fn isLeaseWorkspaceName(name: []const u8) bool {
    if (name.len != UUID_TEXT_LEN) return false;
    for (name, 0..) |c, i| {
        if (std.mem.indexOfScalar(usize, &DASH_INDEXES, i) != null) {
            if (c != DASH_CHAR) return false;
        } else if (!std.ascii.isHex(c) or std.ascii.isUpper(c)) {
            // Lowercase only: `id_format` rejects an uppercase spelling rather
            // than normalizing it, so an uppercase name is not one of ours.
            return false;
        }
    }
    return true;
}

/// True when a canonical path is deep enough to plausibly be a storage home. A
/// filesystem root or a single top-level directory (`/tmp`, `/home`, `/var`) is
/// a stray environment value, not a home, and nothing under it is reapable. The
/// shipped default (`/tmp/agentsfleet-runner`) and the production `/var/lib/…`
/// both clear the floor.
fn isSweepablePath(canonical: []const u8) bool {
    if (!std.fs.path.isAbsolute(canonical)) return false;
    var components = std.mem.tokenizeScalar(u8, canonical, PATH_SEPARATOR);
    var depth: usize = 0;
    while (components.next()) |_| {
        depth += 1;
        if (depth >= MIN_HOME_DEPTH) return true;
    }
    return false;
}

const std = @import("std");
const logging = @import("log");
const client_errors = @import("../engine/client_errors.zig");

const Io = std.Io;
const Dir = std.Io.Dir;
const File = std.Io.File;
const log = logging.scoped(.fleet_runner);
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;

/// The daemon's marker and its process-lifetime lock. Both are dot-prefixed, so
/// the sweep's own files are doubly excluded — by the leading dot the per-lease
/// cleanup already honours, and by failing the lease-id shape check.
const SENTINEL_NAME = ".agentsfleet-runner-home";
const LOCK_NAME = ".agentsfleet-runner-home.lock";

/// Canonical dashed UUID text: 32 hex characters plus 4 dashes, at these offsets.
/// Mirrors `types/id_format.zig`'s spelling — the one the control plane mints.
const UUID_TEXT_LEN: usize = 36;
const DASH_CHAR: u8 = '-';
const DASH_INDEXES = [_]usize{ 8, 13, 18, 23 };

const PATH_SEPARATOR: u8 = '/';
/// A home must sit at least this many components below the filesystem root.
const MIN_HOME_DEPTH: usize = 2;

/// Reapable names collected per pass. Sized so the batch is small stack data
/// while a routine boot (a handful of orphans) completes in one pass.
const SWEEP_BATCH: usize = 64;
/// Ceiling on delete-then-rescan passes, so a home being written concurrently
/// cannot spin the boot; the remainder is logged and reaped next boot.
const MAX_SWEEP_PASSES: u32 = 64;
/// Ceiling on entries examined per pass — a directory with millions of unrelated
/// entries must not stall startup.
const MAX_SCANNED_ENTRIES: u32 = 100_000;

// The two classifiers are pure, so their rules are pinned here, adjacent to the
// prose they encode. Everything that needs a real directory — the four claim
// proofs and the sweep itself — lives in the sibling test file.
test {
    _ = @import("storage_home_test.zig");
}

test "isLeaseWorkspaceName accepts the minted spelling and nothing else" {
    try std.testing.expect(isLeaseWorkspaceName("0199a4c1-8f3e-7b21-9c4d-2f6a1e8b7d05"));
    // Uppercase is a different key everywhere an id is text, so `id_format`
    // rejects it rather than folding it — a home entry spelled that way is not ours.
    try std.testing.expect(!isLeaseWorkspaceName("0199A4C1-8F3E-7B21-9C4D-2F6A1E8B7D05"));
    try std.testing.expect(!isLeaseWorkspaceName("0199a4c1-8f3e-7b21-9c4d-2f6a1e8b7d0")); // short
    try std.testing.expect(!isLeaseWorkspaceName("0199a4c18f3e-7b21-9c4d-2f6a1e8b7d055")); // dash moved
    try std.testing.expect(!isLeaseWorkspaceName("0199a4c1-8f3e-7b21-9c4d-2f6a1e8b7dxz")); // non-hex
    try std.testing.expect(!isLeaseWorkspaceName(".bundle-cache"));
    try std.testing.expect(!isLeaseWorkspaceName(""));
}

test "isSweepablePath refuses a root or a single top-level directory" {
    try std.testing.expect(isSweepablePath("/tmp/agentsfleet-runner")); // the shipped default
    try std.testing.expect(isSweepablePath("/var/lib/agentsfleet/runner"));
    try std.testing.expect(!isSweepablePath("/"));
    try std.testing.expect(!isSweepablePath("/tmp"));
    try std.testing.expect(!isSweepablePath("/home"));
    try std.testing.expect(!isSweepablePath("//")); // separators only, no components
    try std.testing.expect(!isSweepablePath("relative/path"));
}
