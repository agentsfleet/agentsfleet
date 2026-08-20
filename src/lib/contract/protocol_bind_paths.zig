//! The daemon-owned side of the sandbox bind boundary: which host paths reach
//! every lease, which the sandbox constructs, and which an operator may never
//! name. Split from `protocol_bind.zig` on the 350-line bound (RULE FLL), along
//! the seam the two halves already had — this file answers "what does the daemon
//! own", `protocol_bind.zig` answers "what may an operator add to it".
//!
//! The path-shape helpers live here rather than beside the validators because
//! the comptime guards below need them, and a second copy would let the guard
//! and the validator disagree about what "contains" means (RULE UFS).

/// System paths the daemon binds read-only into every sandbox: what a lease
/// needs to dial its inference endpoint and run the transport that dials it.
///
/// This list used to be seven broad trees: `/etc`, `/usr`, `/lib`, `/lib64`,
/// `/bin`, `/sbin`, `/opt`. TWO of them carried credentials into every lease,
/// and those two are the ones that stay gone. `/etc` brought the host account
/// database; `/opt` brought the daemon's own installation directory, whose
/// `.env` holds the control-plane token — readable only by the accident that
/// its owning uid is not mapped into the sandbox's user namespace, which one
/// change of deploy user would erase. Neither is needed by anything a lease
/// runs, so removing them costs nothing and closes a real exposure.
///
/// The EXECUTABLE trees are a different question, and the first answer here was
/// wrong. It reasoned that the runner binary is statically linked and rides its
/// own single-file bind — true — and concluded that no executable is needed
/// inside a lease at all. That conclusion holds only if the runner is the sole
/// thing that runs in a lease, and it is not: the lease child runs the NullClaw
/// engine, whose model transport SPAWNS `curl` (ten provider modules reach
/// `sse.curlStream*` / `http_util.curlPost*`), as does the `http_request` tool.
/// With these trees unbound, `curl` and its shared libraries are absent and
/// every lease dies at `execvp` before its first model call.
///
/// The measurement that was said to prove otherwise did not. The self-test's
/// egress check opens a TCP stream and closes it — its own comment reads "this
/// proves reachability, it does not speak a protocol" — and it runs inside the
/// statically-linked runner, never spawning anything. It confirmed the one path
/// that needs no executable, which is why the gap survived review.
///
/// Restoring them retracts the "nothing executable in a lease" property, which
/// was defence-in-depth, and keeps the credential property, which was the
/// actual exposure. Earning the first one back means removing the `curl`
/// dependency (an in-process transport upstream, or a vetted static binary on
/// its own single-file bind) — not asserting it while the engine shells out.
///
/// Lives in the contract layer, not in the runner, because BOTH sides validate
/// an operator list against it: the runner before `buildArgv`, and the control
/// plane at the API boundary — neither trusts the other's check, and a second
/// copy of this list would let the two disagree about what is protected.
/// `sandbox_args.RO_SYSTEM_PATHS` aliases this (RULE UFS: one source).
pub const BASELINE_RO_PATHS = [_][]const u8{
    DANGER_HOST_SSL_CERTS,
    DANGER_HOST_NETWORK_RESOLVER_DIR,
    DANGER_HOST_NETWORK_HOSTS,
    DANGER_HOST_NETWORK_NSSWITCH,
    DANGER_HOST_SYSTEM_CORE_USR,
    DANGER_HOST_SYSTEM_CORE_LIB,
    DANGER_HOST_SYSTEM_CORE_LIB64,
    DANGER_HOST_SYSTEM_CORE_BIN,
    DANGER_HOST_SYSTEM_CORE_SBIN,
};

// ── DANGER_HOST_ — every host path a lease can reach ────────────────────────
//
// EVERY entry in `BASELINE_RO_PATHS` is named here, and every name carries the
// `DANGER_HOST_` prefix, because every one of them is HOST filesystem mounted
// into a sandbox that runs prompt-injectable agent code. The prefix exists so
// no reference site — here, in landlock's derived read set, in the dashboard
// mirror — can read as routine plumbing. Grep `DANGER_HOST_` to see the
// complete lease-reachable host surface in one list.
//
// Each group states what it buys, because "it is in the baseline" is not a
// reason. A path that cannot answer "which lease-side consumer opens this"
// does not belong here — that question is what removed `/opt` (the daemon's
// control-plane token) and the broad `/etc` (the host account database), both
// now refused at compile time above.

/// TLS trust store. The only filesystem input a credentialed dial needs, and
/// the reason a lease can verify its inference endpoint at all. Read-only and
/// public by nature — the lowest-risk entry in this file.
///
/// PLATFORM ASSUMPTION: the Debian-family and Alpine location. Red Hat family
/// hosts keep their bundle under `/etc/pki/tls/certs`, which this does not
/// carry. `SSL_CERT_FILE` / `SSL_CERT_DIR` ride the environment allowlist
/// (`sandbox_env`), so an operator can point the transport at another bundle —
/// but the path they name must ALSO be bound, or the override resolves to
/// nothing inside the lease and every TLS dial fails.
const DANGER_HOST_SSL_CERTS = "/etc/ssl/certs";

/// Name resolution. Three host paths, all read by the libc resolver inside a
/// lease; without them every hostname fails and no model is reachable.
///
/// The resolver DIRECTORY is bound rather than `/etc/resolv.conf` itself:
/// bwrap resolves a symlink when it binds, which would drop the target file
/// into an `/etc` no landlock rule covers, and every lease would lose DNS.
/// `/etc/resolv.conf` is emitted as a symlink into this directory instead
/// (`RESOLV_LINK` below), so the read lands inside a granted path exactly as
/// it does on the host. Measured: binding the resolved file gave
/// `resolver=0 dns=0 egress=0`; the symlink gave all three passing.
///
/// PLATFORM ASSUMPTION, and the sharpest one in this file: this is the
/// **systemd-resolved** layout. It is present on the deploy target (the unit
/// in `deploy/baremetal/` is a systemd service) and absent on Alpine, on
/// containers, and on any host resolving through NetworkManager or a static
/// `/etc/resolv.conf`.
///
/// Absence does NOT degrade gracefully, and that is the part worth knowing.
/// The directory bind is `--ro-bind-try`, so a missing directory is skipped
/// silently — but the symlink is emitted unconditionally, so the lease ends up
/// with a DANGLING `/etc/resolv.conf` and no name resolution at all. Before
/// the narrowing `/etc` was bound wholesale and the host's own `resolv.conf`
/// worked on any layout, so this is a portability REGRESSION, not a
/// pre-existing limit. It surfaces as a red self-test resolver row rather than
/// silently, which is the improvement working — but on a non-systemd host the
/// runner is degraded from first boot. Binding the host's own
/// `/etc/resolv.conf` as a single file when this directory is absent is the
/// fix; it is not written yet.
const DANGER_HOST_NETWORK_RESOLVER_DIR = "/run/systemd/resolve";
/// Static name resolution, consulted before the resolver.
const DANGER_HOST_NETWORK_HOSTS = "/etc/hosts";
/// Name-service switch configuration — the libc reads it to know it may
/// consult DNS at all. A single file, never the `/etc` tree it lives in.
const DANGER_HOST_NETWORK_NSSWITCH = "/etc/nsswitch.conf";

/// System core: the host's executables and shared libraries. The widest
/// surface in this file by a wide margin, and the one carrying real risk.
///
/// What it costs: `/usr` alone is tens of thousands of files — every
/// interpreter, every system utility, every installed package's data — all
/// readable and executable by agent code inside a lease. Landlock grants
/// exactly what bwrap binds, so nothing downstream narrows it further.
///
/// Why it is here anyway: the NullClaw engine's model transport SPAWNS `curl`
/// (ten provider modules reach `sse.curlStream*` / `http_util.curlPost*`), as
/// does the `http_request` tool. Without these trees `curl` and its shared
/// libraries do not exist inside a lease, and every lease dies at `execvp`
/// before its first model call. They buy a WORKING PRODUCT, not security.
///
/// What removes them: making the transport need no subprocess — an in-process
/// HTTP client upstream, or a vetted static `curl` on its own single-file
/// bind. Until one lands this group is a standing debt, and the prefix is here
/// so it reads as one every time somebody touches this list.
const DANGER_HOST_SYSTEM_CORE_USR = "/usr";
const DANGER_HOST_SYSTEM_CORE_LIB = "/lib";
const DANGER_HOST_SYSTEM_CORE_LIB64 = "/lib64";
const DANGER_HOST_SYSTEM_CORE_BIN = "/bin";
const DANGER_HOST_SYSTEM_CORE_SBIN = "/sbin";

// Every baseline entry must come from a `DANGER_HOST_` constant above — a bare
// path literal appended to the list is refused at compile time.
//
// The prefix is the whole control. A reviewer scanning `BASELINE_RO_PATHS`
// should not have to know which of its entries is a broad host tree and which
// is one harmless file; the NAME says so at the definition and at every
// reference. Without this guard the rule survives exactly as long as whoever
// adds the next path remembers it, which is how the six trees accumulated in
// the first place.
comptime {
    const named = [_][]const u8{
        DANGER_HOST_SSL_CERTS,
        DANGER_HOST_NETWORK_RESOLVER_DIR,
        DANGER_HOST_NETWORK_HOSTS,
        DANGER_HOST_NETWORK_NSSWITCH,
        DANGER_HOST_SYSTEM_CORE_USR,
        DANGER_HOST_SYSTEM_CORE_LIB,
        DANGER_HOST_SYSTEM_CORE_LIB64,
        DANGER_HOST_SYSTEM_CORE_BIN,
        DANGER_HOST_SYSTEM_CORE_SBIN,
    };
    if (named.len != BASELINE_RO_PATHS.len)
        @compileError("BASELINE_RO_PATHS has an entry with no DANGER_HOST_ constant — name it, so the risk is legible at every reference site");
    for (BASELINE_RO_PATHS) |p| {
        var found = false;
        for (named) |n| {
            if (std.mem.eql(u8, n, p)) found = true;
        }
        if (!found)
            @compileError("BASELINE_RO_PATHS entry is not a DANGER_HOST_ constant: " ++ p);
    }
}

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
pub const RESOLV_LINK_TARGET = DANGER_HOST_NETWORK_RESOLVER_DIR ++ "/stub-resolv.conf";

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

// The two credential-bearing trees stay out, pinned at compile time on every
// platform. Nothing else refuses a quiet re-add: leases keep working and every
// probe check stays green either way, because the exposure is what an injected
// prompt can READ, never anything the runner itself touches.
//
// This is the half of the narrowing that survived contact with reality. The
// executable trees came back (see `BASELINE_RO_PATHS` above — the engine's
// transport spawns `curl`); these two never had a consumer to begin with.
//
// Checked in BOTH directions, because the first version of this guard only
// looked down. Naming `/opt` re-admits the daemon's token directly; naming an
// ANCESTOR of it re-admits it just as completely while matching no exact name.
// `/` is called out separately: it is an ancestor of everything but contains no
// path by the segment rule below, so it would slip a containment test.
comptime {
    for (BASELINE_RO_PATHS) |p| {
        if (std.mem.eql(u8, p, "/"))
            @compileError("BASELINE_RO_PATHS may not name the filesystem root");

        // `/opt` holds the daemon's install directory and its `.env`. No path
        // under it has a lease-side consumer, so the whole subtree is refused.
        if (std.mem.eql(u8, p, "/opt") or containsPath("/opt", p) or containsPath(p, "/opt"))
            @compileError("credential-bearing /opt back in BASELINE_RO_PATHS: " ++ p);

        // `/etc` holds the host account database. Unlike `/opt` the tree DOES
        // have lease-side consumers, so specific files under it stay legal
        // (`/etc/ssl/certs`, `/etc/hosts`, `/etc/nsswitch.conf` above) and only
        // the whole tree — or an ancestor that would re-admit it — is refused.
        if (std.mem.eql(u8, p, "/etc") or containsPath(p, "/etc"))
            @compileError("broad /etc back in BASELINE_RO_PATHS — bind the specific file instead: " ++ p);
    }
}

/// Paths an operator bind may never name, beyond the baseline itself. Two
/// groups: mounts the sandbox already establishes (`/usr`, `/proc`,
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
/// refused by overlap; narrowing it to the individual files a lease actually
/// reads opened `/etc/shadow` — and, worse,
/// `/etc/resolv.conf`, which is no longer a bind at all but a symlink
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
