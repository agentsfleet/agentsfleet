//! landlock_policy.zig — WHAT a lease may touch, as sets.
//!
//! Split from `landlock.zig` on the 350-line bound (RULE FLL), along the seam
//! the file already had: this half declares the kernel's access bits, the masks
//! derived from them, and the path lists each mask is granted on; the other
//! half is the syscall mechanics that hand those sets to the kernel. The two
//! change for different reasons — a new lease-reachable path edits this file
//! and nothing else, and the Landlock ABI edits the other.
//!
//! The set assertions live here with the sets, so a rule and its proof are read
//! together and neither can move without the other.

const std = @import("std");
const protocol = @import("contract").protocol;

// Landlock access flags for filesystem (ABI v1).
pub const LANDLOCK_ACCESS_FS_EXECUTE: u64 = 1 << 0;
pub const LANDLOCK_ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
pub const LANDLOCK_ACCESS_FS_READ_FILE: u64 = 1 << 2;
pub const LANDLOCK_ACCESS_FS_READ_DIR: u64 = 1 << 3;
pub const LANDLOCK_ACCESS_FS_REMOVE_DIR: u64 = 1 << 4;
pub const LANDLOCK_ACCESS_FS_REMOVE_FILE: u64 = 1 << 5;
pub const LANDLOCK_ACCESS_FS_MAKE_CHAR: u64 = 1 << 6;
pub const LANDLOCK_ACCESS_FS_MAKE_DIR: u64 = 1 << 7;
pub const LANDLOCK_ACCESS_FS_MAKE_REG: u64 = 1 << 8;
pub const LANDLOCK_ACCESS_FS_MAKE_SOCK: u64 = 1 << 9;
pub const LANDLOCK_ACCESS_FS_MAKE_FIFO: u64 = 1 << 10;
pub const LANDLOCK_ACCESS_FS_MAKE_BLOCK: u64 = 1 << 11;
pub const LANDLOCK_ACCESS_FS_MAKE_SYM: u64 = 1 << 12;

pub const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;

// Full set of handled access rights for ruleset creation.
pub const ALL_FS_ACCESS: u64 = LANDLOCK_ACCESS_FS_EXECUTE |
    LANDLOCK_ACCESS_FS_WRITE_FILE |
    LANDLOCK_ACCESS_FS_READ_FILE |
    LANDLOCK_ACCESS_FS_READ_DIR |
    LANDLOCK_ACCESS_FS_REMOVE_DIR |
    LANDLOCK_ACCESS_FS_REMOVE_FILE |
    LANDLOCK_ACCESS_FS_MAKE_CHAR |
    LANDLOCK_ACCESS_FS_MAKE_DIR |
    LANDLOCK_ACCESS_FS_MAKE_REG |
    LANDLOCK_ACCESS_FS_MAKE_SOCK |
    LANDLOCK_ACCESS_FS_MAKE_FIFO |
    LANDLOCK_ACCESS_FS_MAKE_BLOCK |
    LANDLOCK_ACCESS_FS_MAKE_SYM;

// Workspace gets full RW access.
pub const WORKSPACE_ACCESS: u64 = LANDLOCK_ACCESS_FS_READ_FILE |
    LANDLOCK_ACCESS_FS_WRITE_FILE |
    LANDLOCK_ACCESS_FS_READ_DIR |
    LANDLOCK_ACCESS_FS_MAKE_REG |
    LANDLOCK_ACCESS_FS_MAKE_DIR |
    LANDLOCK_ACCESS_FS_REMOVE_FILE |
    LANDLOCK_ACCESS_FS_REMOVE_DIR |
    LANDLOCK_ACCESS_FS_MAKE_SYM;

// System paths get read-only + execute.
pub const SYSTEM_READONLY_ACCESS: u64 = LANDLOCK_ACCESS_FS_READ_FILE |
    LANDLOCK_ACCESS_FS_READ_DIR |
    LANDLOCK_ACCESS_FS_EXECUTE;

/// The rights landlock accepts on a NON-directory (`ACCESS_FILE` in the
/// kernel's `fs/landlock/syscalls.c`). A rule whose access carries anything
/// outside this set is refused WHOLESALE with `EINVAL` when the target is not
/// a directory — the rule never lands, so the path stays bound and unreadable.
///
/// This matters because the narrowed baseline is the first read set to name a
/// regular FILE: `/etc/hosts`. Every earlier entry (`/etc`, `/usr`, `/lib`,
/// `/run/systemd/resolve`) was a directory, so `READ_DIR` rode along harmlessly
/// and this constraint never fired.
pub const FILE_ONLY_ACCESS: u64 = LANDLOCK_ACCESS_FS_READ_FILE |
    LANDLOCK_ACCESS_FS_WRITE_FILE |
    LANDLOCK_ACCESS_FS_EXECUTE;

/// Read-only paths landlock needs beyond the bind list's baseline: the sandbox
/// floor bwrap constructs (devtmpfs, proc) rather than bind-declares, so the
/// shared lists do not carry them. The writable tmpfs floor is NOT here — it
/// takes `WORKSPACE_ACCESS` below, from the same shared list bwrap mounts it
/// from.
///
/// `/usr` is NOT listed here even though a lease reads it: it arrives through
/// `BASELINE_RO_PATHS` instead, so the mount layer and the policy layer take it
/// from the same source. A second entry here would be the exact drift this
/// derivation exists to prevent.
pub const LANDLOCK_FLOOR_RO_PATHS = [_][]const u8{ "/dev", "/proc" };

/// Device files a lease WRITES, granted per FILE on top of the read-only floor
/// above. `--dev` builds a devtmpfs where these are writable at the MOUNT
/// layer; without this list the policy layer left them read-only, and the two
/// layers disagreeing is the fault class this derivation exists to remove — the
/// third instance of it, after the resolver bind and the child's HOME.
///
/// Measured on `zombie-dev-worker-ant`, every lease, at zero wall seconds and
/// zero tokens: `open("/dev/null", O_RDWR) = -1 EACCES`. The engine's model
/// transport spawns `curl`, and the spawn wires an ignored stdio stream through
/// `/dev/null` — so no lease reached its first model call while the self-test
/// reported the host healthy.
///
/// `/dev/null` alone, and NOT a write grant on `/dev`: a directory rule would
/// cover every node `--dev` builds, and nothing else in that devtmpfs is
/// written — `/dev/{zero,random,urandom}` are read, and `--new-session`
/// detaches the terminal so no lease has a `/dev/tty` to write to.
/// Pub so `selftest_probe` grades the same entries this grants, from this one
/// source: a probe iterating its own copy would pass while the policy layer
/// carried a different set, which is the divergence in miniature.
pub const LANDLOCK_FLOOR_RW_FILES = [_][]const u8{"/dev/null"};

/// System paths that get read-only access in the sandbox. Derived from the
/// bind contract so bwrap and landlock can never disagree on what a lease may
/// read: this list once omitted `/run/systemd/resolve` while bwrap bound it,
/// so `open("/etc/resolv.conf")` followed the symlink into a landlock-denied
/// target and every lease's DNS died — while the self-test, which did not
/// apply landlock, reported the resolver healthy.
pub const SYSTEM_READONLY_PATHS = protocol.BASELINE_RO_PATHS ++ LANDLOCK_FLOOR_RO_PATHS;

test "the read-only floor mask cannot write, so a written device file needs its own rule" {
    // The M136 incident this list closes, stated as the two facts that produced
    // it. `/dev` rides the read-only floor, whose mask has no WRITE_FILE, so
    // `open("/dev/null", O_RDWR)` returned EACCES on every lease while bwrap's
    // `--dev` had it writable at the mount layer. Both halves have to hold for
    // the per-file rule to be the fix: drop the first and `/dev` was already
    // writable, drop the second and the new rule grants nothing.
    try std.testing.expectEqual(@as(u64, 0), SYSTEM_READONLY_ACCESS & LANDLOCK_ACCESS_FS_WRITE_FILE);
    try std.testing.expect(FILE_ONLY_ACCESS & LANDLOCK_ACCESS_FS_WRITE_FILE != 0);
}

test "every writable device file nests under a floor directory bwrap constructs" {
    // The grant is additive on top of a directory the sandbox already builds,
    // never a standalone path. An entry outside the floor would be a rule on
    // something no `--dev`/`--proc` ever created: the rule would fail to open,
    // `applyPolicy` would fail closed, and no lease would run at all.
    for (LANDLOCK_FLOOR_RW_FILES) |file| {
        var nested = false;
        for (LANDLOCK_FLOOR_RO_PATHS) |dir| {
            if (file.len > dir.len + 1 and
                std.mem.startsWith(u8, file, dir) and
                file[dir.len] == '/') nested = true;
        }
        try std.testing.expect(nested);
    }
}

test "the writable device set names files, never the floor directory itself" {
    // A directory here would hand every lease write on every node `--dev`
    // builds — the wide fix this narrow one was chosen over. Pinned because the
    // cheap way to silence a future denial in that tree is exactly that edit.
    for (LANDLOCK_FLOOR_RW_FILES) |file| {
        for (LANDLOCK_FLOOR_RO_PATHS) |dir| {
            try std.testing.expect(!std.mem.eql(u8, file, dir));
        }
    }
}

test "the system read mask carries a directory right a regular file cannot take" {
    // Why `addPathRule` retries at all. Landlock refuses a rule on a
    // NON-directory whose access carries any right outside the kernel's
    // ACCESS_FILE set, and `READ_DIR` is outside it — so the full read mask is
    // rejected WHOLESALE (EINVAL) on a regular file. That refusal was being
    // swallowed by `catch continue`, leaving `/etc/hosts` bind-mounted and
    // unreadable with every list test still green.
    //
    // All three halves matter. Drop the first and the retry is dead code; drop
    // the second and the retry cannot succeed; drop the third and the retry
    // lands a rule that grants nothing, which is unreadable by another name.
    try std.testing.expect(SYSTEM_READONLY_ACCESS & LANDLOCK_ACCESS_FS_READ_DIR != 0);
    try std.testing.expectEqual(@as(u64, 0), FILE_ONLY_ACCESS & LANDLOCK_ACCESS_FS_READ_DIR);
    try std.testing.expect(FILE_ONLY_ACCESS & LANDLOCK_ACCESS_FS_READ_FILE != 0);
}

test "the baseline read set names at least one regular file" {
    // The pair to the mask test above: the retry exists because the read set
    // contains FILES, not only directories. `/etc/hosts` was the first, and
    // before it every entry was a directory — which is exactly why the mask
    // was wrong for years without failing anything.
    var has_regular_file = false;
    for (protocol.BASELINE_RO_PATHS) |p| {
        if (std.mem.eql(u8, p, "/etc/hosts")) has_regular_file = true;
        if (std.mem.eql(u8, p, "/etc/nsswitch.conf")) has_regular_file = true;
    }
    try std.testing.expect(has_regular_file);
}

test "landlock read set contains every bind-contract path" {
    // The derivation is comptime, but this pins the PROPERTY the M136 incident
    // violated: a path bwrap binds read-only is never landlock-denied.
    for (protocol.BASELINE_RO_PATHS) |contract_path| {
        var found = false;
        for (SYSTEM_READONLY_PATHS) |p| {
            if (std.mem.eql(u8, p, contract_path)) found = true;
        }
        try std.testing.expect(found);
    }
}

test "landlock write set contains every writable-floor path" {
    // The write-side twin of the read-set pin below: a path bwrap mounts
    // writable is never demoted to read-only by the policy layer. (That every
    // floor entry is operator-unbindable is enforced at comptime in
    // protocol_bind.zig — a runtime arm for it here could never fire.)
    for (protocol.BASELINE_RW_TMPFS) |rw| {
        for (SYSTEM_READONLY_PATHS) |ro| {
            try std.testing.expect(!std.mem.eql(u8, ro, rw));
        }
    }
}
