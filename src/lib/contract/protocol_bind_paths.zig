//! The daemon-owned side of the sandbox bind boundary: which host paths reach
//! every lease, which the sandbox constructs, and which an operator may never
//! name. Split from `protocol_bind.zig` on the 350-line bound (RULE FLL), along
//! the seam the two halves already had — this file answers "what does the daemon
//! own", `protocol_bind.zig` answers "what may an operator add to it".
//!
//! The path-shape helpers live here rather than beside the validators because
//! the comptime guards below need them, and a second copy would let the guard
//! and the validator disagree about what "contains" means (RULE UFS).

/// System paths the daemon binds read-only into every sandbox — the three a
/// lease needs to dial its inference endpoint, and nothing else.
///
/// This list used to be six broad trees: `/etc`, `/usr`, `/lib`, `/lib64`,
/// `/bin`, `/sbin`, `/opt`. Two of them carried credentials into every lease.
/// `/etc` brought the host account database; `/opt` brought the daemon's own
/// installation directory, whose `.env` holds the control-plane token — readable
/// only by the accident that its owning uid is not mapped into the sandbox's
/// user namespace, which one change of deploy user would erase.
///
/// The trees cost nothing to remove because the runner binary is statically
/// linked and is bound separately as a single file: no interpreter, no shared
/// library, and — once no hosted tool can spawn a process — no executable is
/// needed inside a lease at all. Measured on a real host: every executable path
/// answers `execvp: No such file or directory` inside a lease, while the
/// inference dial still succeeds.
///
/// Lives in the contract layer, not in the runner, because BOTH sides validate
/// an operator list against it: the runner before `buildArgv`, and the control
/// plane at the API boundary — neither trusts the other's check, and a second
/// copy of this list would let the two disagree about what is protected.
/// `sandbox_args.RO_SYSTEM_PATHS` aliases this (RULE UFS: one source).
pub const BASELINE_RO_PATHS = [_][]const u8{
    // The TLS trust store — the only filesystem input a credentialed dial needs.
    "/etc/ssl/certs",
    // The directory the resolver symlink resolves into. `/etc/resolv.conf` is
    // NOT bound: bwrap resolves a symlink when it binds, which would drop the
    // target file into an `/etc` no landlock rule covers, and every lease would
    // lose DNS. It is emitted as a symlink instead (`RESOLV_LINK` below), so the
    // read lands inside this granted directory exactly as it does on the host.
    "/run/systemd/resolve",
    // Static name resolution, consulted before the resolver.
    "/etc/hosts",
};

/// Paths bwrap constructs as a fresh private tmpfs in every sandbox — writable
/// at the mount layer, per-lease, gone at exit. Both enforcement layers consume
/// this one list (`sandbox_args` emits a `--tmpfs` per entry; landlock grants
/// write), so mount and policy can never disagree about where a lease may
/// write — the write-side twin of the resolver drift `BASELINE_RO_PATHS`
/// exists to prevent. The engine writes credentialed dial headers here, so a
/// floor entry landlock demotes to read-only fails every lease at its first
/// credentialed call.
pub const BASELINE_RW_TMPFS = [_][]const u8{"/tmp"};

/// The resolver symlink the sandbox recreates, and the target it points at.
/// A LINK rather than a bind, and the distinction is load-bearing: measured on a
/// real host, binding the resolved file gave `resolver=0 dns=0 egress=0` under
/// landlock while the identical mount set with this symlink gave all three
/// passing. Landlock grants the resolver DIRECTORY; a symlink into it inherits
/// that grant, a bind-mounted copy sitting in an ungranted `/etc` does not.
///
/// The target is fixed rather than read from the host so `buildArgv` stays a
/// pure function of the policy — the same reason its composition is unit-tested
/// on hosts without bubblewrap. A host with no systemd-resolved has no
/// `/run/systemd/resolve` to bind either, so the baseline already assumes this.
pub const RESOLV_LINK = "/etc/resolv.conf";
pub const RESOLV_LINK_TARGET = "/run/systemd/resolve/stub-resolv.conf";

/// The sandboxed child's `HOME`. The daemon's own `HOME` is deliberately NOT
/// forwarded: the unit sets it to its `RuntimeDirectory` (`/run/agentsfleet`),
/// a host path no bind list carries and no landlock rule covers, so the engine
/// resolved its configuration directory onto a path that answers `EACCES`. Every
/// dev lease died there as `AccessDenied`, at zero wall seconds, before its first
/// model call — and the mount layer was innocent: bwrap builds a writable tmpfs at
/// `/run` for the resolver bind's mountpoint, so the `mkdir` SUCCEEDS and only the
/// policy layer refuses.
///
/// A path on the writable floor closes it from both sides at once: bwrap builds
/// the tmpfs per lease, landlock grants it write from that same list, and the
/// directory dies with the lease rather than accumulating agent state on the host.
pub const CHILD_HOME = "/tmp/agentsfleet-home";

// The child's home must sit INSIDE the writable floor. Outside it, this constant
// would reintroduce exactly the fault it exists to close — a home the mount layer
// never builds and the policy layer never grants.
comptime {
    var inside = false;
    for (BASELINE_RW_TMPFS) |rw| {
        if (containsPath(rw, CHILD_HOME)) inside = true;
    }
    if (!inside)
        @compileError("CHILD_HOME must nest under a BASELINE_RW_TMPFS entry: " ++ CHILD_HOME);
}

// The resolver link is only as good as the directory it lands in: a target
// outside the baseline is a target landlock does not cover, and DNS dies exactly
// as it did when this was a bind.
comptime {
    var covered = false;
    for (BASELINE_RO_PATHS) |p| {
        if (containsPath(p, RESOLV_LINK_TARGET)) covered = true;
    }
    if (!covered)
        @compileError("RESOLV_LINK_TARGET must nest under a BASELINE_RO_PATHS entry: " ++ RESOLV_LINK_TARGET);
}

/// Paths an operator bind may never name, beyond the baseline itself. Two
/// groups: mounts the bwrap base argv already establishes (`/usr`, `/proc`,
/// `/dev`, `/tmp` — re-binding one changes the sandbox's own floor), and host
/// surfaces where a WRITABLE mount is host control rather than a repair
/// (`/root`, `/home`, `/boot`, `/sys`, `/run`, `/var/run` and BOTH of the
/// daemon's own state directories — the last two cover the runner token and the
/// container socket).
///
/// Both, because the list named only `/var/lib/agentsfleet` while the deploy
/// writes the token to `/opt/agentsfleet/.env`. The protection named a directory
/// the token does not live in. That went unnoticed while `/opt` sat in the
/// baseline — an operator could not bind what the daemon already bound — so
/// narrowing the baseline is exactly what makes naming it here load-bearing.
///
/// `/etc` is here for the same reason and was caught the same way. While the
/// whole tree sat in the baseline, an operator bind anywhere under it was
/// refused by overlap; narrowing to three files opened `/etc/shadow` — and,
/// worse, `/etc/resolv.conf`, which is no longer a bind at all but a symlink
/// the base argv emits. Operator binds are appended AFTER that argv and bwrap's
/// last operation on a target wins, so a bind there would replace the resolver
/// link and redirect name resolution for every lease this runner takes. Naming
/// the tree restores exactly the refusal the baseline used to provide by
/// accident, and costs nothing: no operator bind under `/etc` was ever allowed.
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
    "/run",  "/var/run", "/var/lib/agentsfleet", "/opt/agentsfleet",
    "/etc",
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

/// True when two absolute paths name the same mount or one contains the other.
/// Segment-aware: `/etc` contains `/etc/ssl` but NOT `/etcetera`, so a prefix
/// compare alone would refuse legitimate paths and admit nothing extra.
///
/// Compares the strings as given, so it is only sound on canonical paths.
/// `extraBindsValid` runs `bindPathValid` first for exactly that reason — the
/// two are one check split in half, not two independent ones.
pub fn pathsOverlap(a: []const u8, b: []const u8) bool {
    if (std.mem.eql(u8, a, b)) return true;
    return containsPath(a, b) or containsPath(b, a);
}

/// True when `parent` contains `child` as a directory subtree.
pub fn containsPath(parent: []const u8, child: []const u8) bool {
    if (child.len <= parent.len) return false;
    if (!std.mem.startsWith(u8, child, parent)) return false;
    return child[parent.len] == '/';
}

const std = @import("std");
