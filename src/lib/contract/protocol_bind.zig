//! Sandbox bind contract — which host paths reach a lease's sandbox, at what
//! mode, and which an operator may name. Split from `protocol_policy.zig`
//! (RULE FLL) and re-exported through `protocol.zig`, so consumers keep the
//! `protocol.X` names.
//!
//! Two layers, and the split is the security property: a daemon-owned baseline
//! that no assignment can touch, plus an operator list that may only APPEND to
//! it. The runner composes them before `buildArgv`; the control plane validates
//! the same list at the API boundary. Neither side trusts the other's check,
//! which is why the paths and the validator live here rather than in the runner.

/// How one host path is mounted into a lease's sandbox. `read_write` lets the
/// agent's own code modify host state outside its workspace, on every lease
/// that runner takes — an operator opts into that per path, and an unstated
/// mode is always `read_only` so a malformed or older assignment can never
/// widen access by omission.
pub const BindMode = enum {
    read_only,
    read_write,

    /// The bwrap flag this mode emits. `-try` on both: a path absent on this
    /// host is skipped rather than failing the lease, and shows up as a failed
    /// self-test check instead of a dead runner.
    pub fn bwrapFlag(self: BindMode) []const u8 {
        return switch (self) {
            .read_only => "--ro-bind-try",
            .read_write => "--bind-try",
        };
    }

    /// Operator-facing label — the dashboard and the self-test name the mode
    /// the same way, so "why can the agent write here" is answerable from
    /// either surface (RULE UFS: single source for both runtimes).
    pub fn label(self: BindMode) []const u8 {
        return switch (self) {
            .read_only => "read-only",
            .read_write => "read-write",
        };
    }
};

/// One operator-assigned mount: the host path, how it is mounted, and the
/// operator's own note saying why it exists. The note is carried so the
/// self-test can echo it back per check — an unexplained mount on a security
/// boundary is how a bind outlives the reason it was added.
pub const ExtraBind = struct {
    path: []const u8,
    mode: BindMode = .read_only,
    note: []const u8 = "",
};

/// System paths the daemon binds read-only into every sandbox. Lives in the
/// contract layer, not in the runner, because BOTH sides validate an operator
/// list against it: the runner before `buildArgv`, and the control plane at the
/// API boundary — neither trusts the other's check, and a second copy of this
/// list would let the two disagree about what is protected.
/// `sandbox_args.RO_SYSTEM_PATHS` aliases this (RULE UFS: one source).
pub const BASELINE_RO_PATHS = [_][]const u8{ "/etc", "/lib", "/lib64", "/bin", "/sbin", "/opt", "/run/systemd/resolve" };

/// Paths bwrap constructs as a fresh private tmpfs in every sandbox — writable
/// at the mount layer, per-lease, gone at exit. Both enforcement layers consume
/// this one list (`sandbox_args` emits a `--tmpfs` per entry; landlock grants
/// write), so mount and policy can never disagree about where a lease may
/// write — the write-side twin of the resolver drift `BASELINE_RO_PATHS`
/// exists to prevent. The engine writes credentialed dial headers here, so a
/// floor entry landlock demotes to read-only fails every lease at its first
/// credentialed call.
pub const BASELINE_RW_TMPFS = [_][]const u8{"/tmp"};

/// Paths an operator bind may never name, beyond the baseline itself. Two
/// groups: mounts the bwrap base argv already establishes (`/usr`, `/proc`,
/// `/dev`, `/tmp` — re-binding one changes the sandbox's own floor), and host
/// surfaces where a WRITABLE mount is host control rather than a repair
/// (`/root`, `/home`, `/boot`, `/sys`, `/run`, `/var/run` and the daemon's own
/// state dir — the last covers the runner token and the container socket).
///
/// This is a backstop, not the security model. A denylist alone fails open on
/// everything unlisted, so the load-bearing controls stay structural: overlap
/// with a protected path is refused outright, the mode is explicit and defaults
/// closed, and the self-test reports every entry. The list exists because
/// `read_write` is assignable, and the cost of missing the obvious host-control
/// paths is a runner-wide escalation rather than one failed lease.
pub const SENSITIVE_PATHS = [_][]const u8{
    "/usr",  "/proc",    "/dev",                 "/tmp",
    "/root", "/home",    "/boot",                "/sys",
    "/run",  "/var/run", "/var/lib/agentsfleet",
};

// A mount the sandbox constructs is never an operator's to re-mode: every
// writable-floor path must sit in the refusal list above, or an extra bind
// could shadow the per-lease tmpfs with a host directory.
comptime {
    for (BASELINE_RW_TMPFS) |rw| {
        var protected = false;
        for (SENSITIVE_PATHS) |sp| {
            if (std.mem.eql(u8, rw, sp)) protected = true;
        }
        if (!protected) @compileError("BASELINE_RW_TMPFS entry missing from SENSITIVE_PATHS: " ++ rw);
    }
}

/// Extra-bind bounds. The operator list is ADDITIVE to the daemon-owned
/// baseline: an assignment can only append, never drop or re-mode a path the
/// sandbox depends on.
pub const MAX_EXTRA_BINDS: usize = 16;
pub const MAX_BIND_PATH_LEN: usize = 4096; // PATH_MAX on Linux
pub const MAX_BIND_NOTE_LEN: usize = 200; // one line of operator intent, not a document

/// Every operator-supplied bind must carry a well-formed absolute path, a
/// bounded note, and no overlap with a path the daemon already binds. Fails the
/// WHOLE list on one bad entry: a partially applied bind set is a sandbox
/// nobody reasoned about, so the caller degrades the runner instead of leasing.
///
/// A denylist alone would fail open on everything unlisted and go stale the
/// moment a host gains a new sensitive path — the same rot that left
/// `/run/systemd/resolve` out of the baseline and broke every lease. So the
/// primary control is structural and closed by default: an entry that OVERLAPS
/// a protected path in either direction is refused, which is what actually
/// keeps the list additive.
///
/// Overlap, not equality. bwrap applies bind operations in argv order and the
/// last operation on a target wins, so an operator entry is appended AFTER the
/// baseline and would otherwise re-mode it. Three shapes all re-mode:
///   `/etc`      names a baseline path outright
///   `/etc/ssl`  nests under one, re-moding that subtree
///   `/run`      CONTAINS `/run/systemd/resolve`, shadowing it wholesale
/// Refusing all three is what makes "additive, never re-moded" true of the
/// built argv rather than only of the composed array.
pub fn extraBindsValid(binds: []const ExtraBind) bool {
    if (binds.len > MAX_EXTRA_BINDS) return false;
    for (binds) |b| {
        if (!bindPathValid(b.path)) return false;
        if (b.note.len > MAX_BIND_NOTE_LEN) return false;
        if (pathOverlapsProtected(b.path)) return false;
    }
    return true;
}

/// True when `path` names, nests under, or contains a path the daemon binds or
/// refuses. Split out of `extraBindsValid` so the RUNNER can ask the same
/// question of a path it has resolved on its own host.
///
/// That split is the whole point. Every check in `extraBindsValid` is lexical,
/// and it has to be — the control plane validates an assignment from a machine
/// where the operator's paths do not exist, so a string is all it has. But
/// bubblewrap resolves symlinks itself when it opens a bind source, so a
/// declared `/srv/shared` that links to `/etc` satisfies every string check and
/// still mounts the host's `/etc` into the lease, writable if the assignment
/// said so. Only the runner can close that, and it closes it by asking this
/// function about the resolved path (`sandbox_args.assertBindTargetsSafe`).
pub fn pathOverlapsProtected(path: []const u8) bool {
    for (BASELINE_RO_PATHS) |p| {
        if (pathsOverlap(path, p)) return true;
    }
    for (SENSITIVE_PATHS) |p| {
        if (pathsOverlap(path, p)) return true;
    }
    return false;
}

/// True when two absolute paths name the same mount or one contains the other.
/// Segment-aware: `/etc` contains `/etc/ssl` but NOT `/etcetera`, so a prefix
/// compare alone would refuse legitimate paths and admit nothing extra.
///
/// Compares the strings as given, so it is only sound on canonical paths.
/// `extraBindsValid` runs `bindPathValid` first for exactly that reason — the
/// two are one check split in half, not two independent ones.
fn pathsOverlap(a: []const u8, b: []const u8) bool {
    if (std.mem.eql(u8, a, b)) return true;
    return containsPath(a, b) or containsPath(b, a);
}

/// True when `parent` contains `child` as a directory subtree.
fn containsPath(parent: []const u8, child: []const u8) bool {
    if (child.len <= parent.len) return false;
    if (!std.mem.startsWith(u8, child, parent)) return false;
    return child[parent.len] == '/';
}

/// One path's grammar: absolute, already canonical, no NUL, no trailing slash.
/// Canonical is the load-bearing half. `pathsOverlap` compares raw strings, so
/// any path admitted here that RESOLVES somewhere other than where it reads is
/// a hole straight through the overlap check: `/etc/./ssl` matches neither
/// `/etc` nor `/etc/ssl` textually, yet bwrap binds it onto the baseline's
/// `/etc/ssl` — at `read_write` if the operator asked, on every lease that
/// runner takes. Refusing the non-canonical spelling is what keeps the string
/// compare and the kernel's resolution talking about the same mount.
fn bindPathValid(path: []const u8) bool {
    if (path.len < 2 or path.len > MAX_BIND_PATH_LEN) return false;
    if (path[0] != '/') return false; // absolute only — no cwd-relative mounts
    if (path[path.len - 1] == '/') return false; // one spelling per path
    if (std.mem.indexOfScalar(u8, path, 0) != null) return false; // NUL truncation
    var it = std.mem.splitScalar(u8, path, '/');
    _ = it.next(); // the leading '/' always yields one empty segment
    while (it.next()) |seg| {
        if (seg.len == 0) return false; // `//` — same mount, second spelling
        if (std.mem.eql(u8, seg, ".")) return false; // no-op segment, same
        if (std.mem.eql(u8, seg, "..")) return false; // no escaping the named root
    }
    return true;
}

const std = @import("std");

test "test_operator_bind_validation_refuses_unsafe_paths" {
    // The grammar an operator-supplied mount must satisfy.
    const ok = [_]ExtraBind{
        .{ .path = "/srv/fonts" },
        .{ .path = "/srv/models", .mode = .read_write, .note = "shared model cache" },
    };
    try std.testing.expect(extraBindsValid(&.{})); // empty = baseline only
    try std.testing.expect(extraBindsValid(&ok));

    try std.testing.expect(!extraBindsValid(&.{.{ .path = "relative/path" }})); // not absolute
    try std.testing.expect(!extraBindsValid(&.{.{ .path = "/srv/../root" }})); // traversal
    try std.testing.expect(!extraBindsValid(&.{.{ .path = "/srv/data/" }})); // trailing slash
    try std.testing.expect(!extraBindsValid(&.{.{ .path = "/" }})); // under the 2-char floor
    try std.testing.expect(!extraBindsValid(&.{.{ .path = "/srv\x00/etc" }})); // NUL truncation
    try std.testing.expect(!extraBindsValid(&.{.{ .path = "/" ++ "a" ** MAX_BIND_PATH_LEN }})); // over length
    try std.testing.expect(!extraBindsValid(&.{.{ .path = "/srv/x", .note = "n" ** (MAX_BIND_NOTE_LEN + 1) }}));

    // Overlap with a path the daemon already binds is refused at BOTH modes.
    // bwrap applies binds in argv order and the last operation on a target
    // wins, so an appended entry naming a baseline path would re-mode the
    // daemon's own mount — read_write turning the sandbox's `/etc` writable.
    // An entry naming one is redundant at best and an escalation at worst.
    for (BASELINE_RO_PATHS) |p| {
        try std.testing.expect(!extraBindsValid(&.{.{ .path = p }}));
        try std.testing.expect(!extraBindsValid(&.{.{ .path = p, .mode = .read_write }}));
    }
    for (SENSITIVE_PATHS) |p| {
        try std.testing.expect(!extraBindsValid(&.{.{ .path = p }}));
        try std.testing.expect(!extraBindsValid(&.{.{ .path = p, .mode = .read_write }}));
    }

    // Overlap runs BOTH directions, which plain equality would miss.
    try std.testing.expect(!extraBindsValid(&.{.{ .path = "/etc/ssl" }})); // nests under a baseline path
    try std.testing.expect(!extraBindsValid(&.{.{ .path = "/run" }})); // CONTAINS /run/systemd/resolve
    try std.testing.expect(!extraBindsValid(&.{.{ .path = "/var" }})); // CONTAINS /var/lib/agentsfleet

    // Segment-aware, so merely sharing a textual prefix is still allowed — a
    // substring compare would refuse this and quietly shrink the surface.
    try std.testing.expect(extraBindsValid(&.{.{ .path = "/etcetera" }}));
    try std.testing.expect(extraBindsValid(&.{.{ .path = "/srv/models" }}));

    // One bad entry fails the whole list — never a partial bind set.
    try std.testing.expect(!extraBindsValid(&.{
        .{ .path = "/srv/fonts" },
        .{ .path = "relative" },
    }));
    try std.testing.expect(!extraBindsValid(&.{
        .{ .path = "/srv/fonts" },
        .{ .path = "/etc", .mode = .read_write },
    }));
}

test "test_non_canonical_spellings_cannot_smuggle_a_bind_onto_a_protected_path" {
    // The overlap check compares raw strings while bwrap binds where the path
    // RESOLVES. Any spelling admitted here that resolves elsewhere re-modes a
    // baseline mount for every lease on the runner — `/etc/./ssl` reads as
    // neither `/etc` nor `/etc/ssl` and lands on the latter.
    const smuggled = [_][]const u8{
        "/etc/./ssl", // `.` segment — resolves under a baseline path
        "/etc/.", // `.` tail — resolves to `/etc` itself
        "//etc", // doubled separator, same mount
        "/etc//ssl", // interior empty segment
        "/run/./systemd/resolve", // shadows the mount whose absence caused M167
        "/./srv/models", // leading `.` on an otherwise allowed path
    };
    for (smuggled) |p| {
        try std.testing.expect(!extraBindsValid(&.{.{ .path = p }}));
        try std.testing.expect(!extraBindsValid(&.{.{ .path = p, .mode = .read_write }}));
    }

    // The canonical spelling of an allowed path still passes — the rule refuses
    // a second spelling, not the mount.
    try std.testing.expect(extraBindsValid(&.{.{ .path = "/srv/models" }}));

    // A segment that merely STARTS with a dot is a real directory name, not a
    // traversal: refusing it would lock operators out of every dotfile mount.
    try std.testing.expect(extraBindsValid(&.{.{ .path = "/srv/.cache/models" }}));
    try std.testing.expect(extraBindsValid(&.{.{ .path = "/srv/..data" }}));
}

test "test_bind_mode_defaults_closed_and_maps_to_its_bwrap_flag" {
    // An assignment that names a path but no mode must never widen access.
    const defaulted = ExtraBind{ .path = "/srv/models" };
    try std.testing.expectEqual(BindMode.read_only, defaulted.mode);

    // Wire compatibility: a control plane that sends only a path still decodes,
    // and decodes CLOSED.
    const parsed = try std.json.parseFromSlice([]const ExtraBind, std.testing.allocator, "[{\"path\":\"/srv/models\"}]", .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expectEqual(BindMode.read_only, parsed.value[0].mode);

    try std.testing.expectEqualStrings("--ro-bind-try", BindMode.read_only.bwrapFlag());
    try std.testing.expectEqualStrings("--bind-try", BindMode.read_write.bwrapFlag());
    try std.testing.expectEqualStrings("read-only", BindMode.read_only.label());
    try std.testing.expectEqualStrings("read-write", BindMode.read_write.label());
}

test "test_extra_binds_are_bounded" {
    // A runner:write caller must not be able to stuff the per-heartbeat payload.
    var over: [MAX_EXTRA_BINDS + 1]ExtraBind = undefined;
    for (&over) |*slot| slot.* = .{ .path = "/srv/models" };
    try std.testing.expect(!extraBindsValid(&over));

    var at_cap: [MAX_EXTRA_BINDS]ExtraBind = undefined;
    for (&at_cap) |*slot| slot.* = .{ .path = "/srv/models" };
    try std.testing.expect(extraBindsValid(&at_cap));
}
