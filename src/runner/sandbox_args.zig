//! sandbox_args.zig — argv + environment policy for a forked child's exec.
//!
//! `dev_none`: exec the runner's `__execute` mode directly. Sandboxed tiers
//! wrap it in bubblewrap — `--unshare-all` (user/pid/net/ipc/uts/cgroup ns),
//! `--new-session` (detach the controlling terminal), read-only system paths,
//! read-write workspace, `--die-with-parent` (the sandbox dies if the runner
//! does), and network per the assigned network policy. Every argv entry is dup'd
//! into the caller's allocator; the caller frees via `freeArgv` after the fork.
//!
//! The child-environment policy lives in `sandbox_env.zig`; this file owns
//! argv only.

const std = @import("std");
const builtin = @import("builtin");
const build_options = @import("build_options");

const contract = @import("contract");
const Config = @import("daemon/config.zig");
const Policy = @import("network/Policy.zig");
const child_exec = @import("child_exec.zig");
const sandbox_hardening = @import("sandbox_hardening.zig");
const sandbox_bind_guard = @import("sandbox_bind_guard.zig");

const BWRAP_PATHS = [_][]const u8{ "/usr/bin/bwrap", "/usr/local/bin/bwrap" };
/// System paths bound read-only when present (`--ro-bind-try` tolerates absence).
/// `/run/systemd/resolve` carries the systemd-resolved stub `resolv.conf` that
/// `/etc/resolv.conf` symlinks to on a systemd-managed host (`allow_all`'s
/// `--share-net` shares the network namespace, but the mount namespace is still
/// unshared, so the child's `/etc/resolv.conf` is a dangling symlink without this
/// — every outbound DNS lookup fails `HostResolutionFailed` regardless of the
/// assigned network policy). Absent and harmless on a non-systemd-resolved host.
/// `pub` for the sibling contract test, which asserts every entry here reaches
/// the built argv. It must read THIS array — a copy in the test would stay
/// green while the real list rots, which is precisely how the resolver bind
/// went missing.
///
/// Aliases the contract-layer list rather than restating it: the control plane
/// validates an operator's assignment against the same paths at the API
/// boundary, and two copies would let the two sides disagree about what an
/// operator is allowed to re-mode (RULE UFS).
pub const RO_SYSTEM_PATHS = contract.protocol.BASELINE_RO_PATHS;
/// Used at several bind sites (RULE UFS); the rest are single-use bwrap flags
/// whose literal spelling IS bwrap's CLI contract.
const RO_BIND = "--ro-bind";

/// Upper bound on the composed bind set: the daemon-owned baseline plus the
/// most extra binds an assignment may carry. Comptime, so the buffer
/// `composeBinds` fills can never overflow.
pub const MAX_BINDS = RO_SYSTEM_PATHS.len + contract.protocol.MAX_EXTRA_BINDS;

/// The ordered bind set for one lease: the daemon-owned baseline FIRST (always
/// read-only), then the operator's additions with the mode each was assigned.
/// Pure and platform-independent by design — the additive-only invariant is the
/// security property of the operator-editable surface, so it is provable on any
/// host rather than only on a Linux runner where `appendBwrap` emits flags.
///
/// Composition, not substitution: `extra` is appended and never consulted when
/// emitting the baseline, so no assignment can drop a path the sandbox depends
/// on or re-mode one to writable. Over-long input degrades to the baseline
/// alone — unreachable in practice (`extraBindsValid` caps the list before it
/// is stored) and fail-closed if it ever were.
pub fn composeBinds(buf: *[MAX_BINDS]contract.protocol.ExtraBind, extra: []const contract.protocol.ExtraBind) []const contract.protocol.ExtraBind {
    for (RO_SYSTEM_PATHS, 0..) |p, i| buf[i] = .{ .path = p, .mode = .read_only };
    if (extra.len > contract.protocol.MAX_EXTRA_BINDS) return buf[0..RO_SYSTEM_PATHS.len];
    for (extra, 0..) |b, i| buf[RO_SYSTEM_PATHS.len + i] = b;
    return buf[0 .. RO_SYSTEM_PATHS.len + extra.len];
}
/// In-sandbox absolute paths for the parent-rendered resolver files: the
/// static `/etc/hosts` (allowlist names → resolved IPs) and a resolver-less
/// `/etc/resolv.conf`. Bound only when `EgressScope` supplied host-side paths.
const ETC_HOSTS = "/etc/hosts";
const ETC_RESOLV = "/etc/resolv.conf";
/// The `allow_all` posture (assigned explicitly from the dashboard;
/// never a fallback — a missing assignment fails closed, M100 §2) re-shares the host
/// network namespace so the lease has full egress while the filtered-veth
/// enforcement (`allow_list_egress` + `establishEgress`) is unbuilt (lands 2.0.1).
const SHARE_NET = "--share-net";

/// The host-side rendered resolver files an established `EgressScope` produced:
/// absolute paths to the per-lease static `/etc/hosts` and the resolver-less
/// `/etc/resolv.conf`. `null` when egress is not enabled
/// (`deny_all`, dev_none, or not yet established) — then no resolver files are
/// bound and the child keeps its image defaults. Borrowed; not owned here.
pub const EgressFiles = struct {
    hosts_path: []const u8,
    resolv_path: []const u8,
};

/// Build the child's exec argv. Sandboxed tiers prepend a bubblewrap wrapper.
/// Every entry is dup'd into `alloc`; free with `freeArgv`. Errors when a
/// sandboxed tier has no `bwrap` binary — the caller then fails the lease
/// closed (Invariant 7) rather than running unsandboxed.
pub fn buildArgv(io: std.Io, alloc: std.mem.Allocator, cfg: Config, workspace_path: []const u8, egress: ?EgressFiles) ![]const []const u8 {
    var list: std.ArrayList([]const u8) = .empty;
    errdefer freeList(alloc, &list);

    const self_exe = try resolveChildExe(io, alloc);
    defer alloc.free(self_exe);

    const sandboxed = isSandboxed(cfg);
    if (sandboxed) try appendBwrap(io, alloc, &list, self_exe, workspace_path, egress, cfg.network_policy, cfg.extra_binds);

    try dup(alloc, &list, self_exe);
    try dup(alloc, &list, child_exec.SUBCOMMAND);
    if (sandboxed) try dup(alloc, &list, child_exec.SANDBOXED_FLAG);
    const ws_flag = try std.fmt.allocPrint(alloc, FLAG_JOIN_FMT, .{ child_exec.WORKSPACE_FLAG_PREFIX, workspace_path });
    {
        // Scoped so a failed append frees ws_flag exactly once; once appended it
        // is owned by `list` (the outer freeList errdefer), so a later
        // toOwnedSlice failure must not double-free it.
        errdefer alloc.free(ws_flag);
        try list.append(alloc, ws_flag);
    }
    if (sandboxed) try appendBindFlags(alloc, &list, cfg.extra_binds);

    return list.toOwnedSlice(alloc);
}

/// One spelling for every `--flag=value` tail argument the child parses (UFS).
const FLAG_JOIN_FMT = "{s}{s}";

/// Forward the operator binds to the child, mode-explicit, so its landlock
/// ruleset admits the same mounts bwrap just made. Without this bwrap mounts
/// the path, landlock refuses the read, and the assignment fails only at first
/// use inside a lease. `pub` like `composeSandboxPrefix` — `buildArgv` reaches
/// it on Linux only, so the mode mapping would be proven on one host and no other.
pub fn appendBindFlags(alloc: std.mem.Allocator, list: *std.ArrayList([]const u8), extra_binds: []const contract.protocol.ExtraBind) !void {
    for (extra_binds) |b| {
        const prefix = switch (b.mode) {
            .read_only => sandbox_hardening.BIND_RO_FLAG_PREFIX,
            .read_write => sandbox_hardening.BIND_RW_FLAG_PREFIX,
        };
        const flag = try std.fmt.allocPrint(alloc, FLAG_JOIN_FMT, .{ prefix, b.path });
        errdefer alloc.free(flag);
        try list.append(alloc, flag);
    }
}

/// Whether this tier gets a bubblewrap wrapper. `dev_none` and every
/// non-Linux host exec the child directly.
///
/// `pub` for `child_process.forkExec`, which must know whether anything will
/// construct the child's `HOME`: on a wrapped tier bwrap's `--dir` builds it
/// per lease, on a direct-exec tier nothing does, and the environ assigns it
/// either way. One predicate rather than a second copy of the tier test (RULE
/// UFS) — a copy is how the mount layer and the policy layer drifted before.
pub fn isSandboxed(cfg: Config) bool {
    return builtin.os.tag == .linux and cfg.sandbox_tier != .dev_none;
}

/// The bubblewrap wrapper alone — every namespace, mount and network flag a
/// lease gets, ending at the `--` that separates it from the child command.
/// Empty on a non-sandboxed tier. Free with `freeArgv`.
///
/// Exists so the self-test probe runs under the SAME sandbox construction a
/// lease does instead of a parallel one built for testing. That is the whole
/// point of the probe: the M167 incident had a green host check and a dead
/// sandbox, so a probe that assembled its own flags would prove nothing about
/// real work. `buildArgv` and the probe both go through here, and
/// `test_probe_uses_the_lease_argv_builder` asserts the two prefixes are
/// byte-identical for the same policy.
///
/// The probe supplies its own child command rather than re-execing
/// `__execute`: a lease's tail runs the real executor, which a self-test must
/// not do. The sandbox is what has to be identical, not the payload.
pub fn buildSandboxPrefix(io: std.Io, alloc: std.mem.Allocator, cfg: Config, workspace_path: []const u8, egress: ?EgressFiles) ![]const []const u8 {
    var list: std.ArrayList([]const u8) = .empty;
    errdefer freeList(alloc, &list);

    if (isSandboxed(cfg)) {
        const self_exe = try resolveChildExe(io, alloc);
        defer alloc.free(self_exe);
        try appendBwrap(io, alloc, &list, self_exe, workspace_path, egress, cfg.network_policy, cfg.extra_binds);
    }
    return list.toOwnedSlice(alloc);
}

/// The sandbox prefix for a GIVEN bwrap binary and child exe — the same
/// composition `buildSandboxPrefix` performs, minus the two host lookups that
/// make it unrunnable off a configured Linux box. Free with `freeArgv`.
///
/// `pub` for the bind-contract tests, which assert which host paths reach a
/// lease and at what mode. Those assertions are about the argv the daemon
/// composes, not about bubblewrap executing, so they must not require the
/// binary to exist — otherwise they skip everywhere and guard nothing. The
/// real-sandbox execution proof is the integration tier's job (RULE ITF).
pub fn composeSandboxPrefix(alloc: std.mem.Allocator, bwrap: []const u8, self_exe: []const u8, cfg: Config, workspace_path: []const u8, egress: ?EgressFiles) ![]const []const u8 {
    var list: std.ArrayList([]const u8) = .empty;
    errdefer freeList(alloc, &list);
    try appendBwrapAt(alloc, &list, bwrap, self_exe, workspace_path, egress, cfg.network_policy, cfg.extra_binds);
    return list.toOwnedSlice(alloc);
}

/// The child's exec target. Normally the runner's own binary (re-exec into
/// `__execute`). An `executor_provider_stub` build (tests only) redirects to the
/// prebuilt stub exe at `build_options.stub_runner_exe_path` — the integration
/// daemon is a `zig test` binary with no `__execute` dispatch, so the forked
/// child must run a real stub-flagged runner instead. Comptime-false in
/// production: the whole branch (and the env-free path string) vanishes.
/// Pub for `selftest.buildProbeArgv`: the probe's tail execs this same binary
/// in `__selftest_probe` mode, and it must be the byte-identical path the
/// prefix `--ro-bind`s — resolving it a second way would bind one path and exec
/// another.
pub fn resolveChildExe(io: std.Io, alloc: std.mem.Allocator) ![:0]u8 {
    // Match executablePathAlloc's sentinel slice so the caller's single
    // `alloc.free` frees the exact bytes allocated (len + 1).
    if (build_options.executor_provider_stub and build_options.stub_runner_exe_path.len > 0) {
        const stub = build_options.stub_runner_exe_path;
        // The build emits this path relative to the build root. bwrap resolves
        // a BIND source against our cwd but execs the tail against the
        // SANDBOX's cwd, which `--chdir` has already moved to the workspace —
        // so a relative target binds fine and then fails `execvp`. Absolutise
        // it here, the same reason `requireAbsoluteArgv0` refuses a relative
        // argv[0]: the exec target must not depend on who is asking.
        if (std.fs.path.isAbsolute(stub)) return alloc.dupeZ(u8, stub);
        // Resolved against the build root the build baked in, not against the
        // cwd: Zig 0.16 has no portable cwd lookup, and the build root is the
        // frame the emitted path is relative to by construction.
        return std.fs.path.joinZ(alloc, &.{ build_options.build_root, stub });
    }
    return std.process.executablePathAlloc(io, alloc);
}

/// Free an argv produced by `buildArgv`.
pub fn freeArgv(alloc: std.mem.Allocator, argv: []const []const u8) void {
    for (argv) |s| alloc.free(s);
    alloc.free(argv);
}

fn freeList(alloc: std.mem.Allocator, list: *std.ArrayList([]const u8)) void {
    for (list.items) |s| alloc.free(s);
    list.deinit(alloc);
}

fn dup(alloc: std.mem.Allocator, list: *std.ArrayList([]const u8), s: []const u8) !void {
    const copy = try alloc.dupe(u8, s);
    errdefer alloc.free(copy); // freed once here if append fails; else owned by list
    try list.append(alloc, copy);
}

/// One `<flag> <path> <path>` triple, the flag chosen by the bind's own mode.
/// `-try` on both modes tolerates a path absent on this host: a baseline entry
/// that does not exist here is skipped rather than failing the lease, and an
/// operator entry naming a missing directory shows up as a failed self-test
/// check instead of a dead runner.
fn bindTry(alloc: std.mem.Allocator, list: *std.ArrayList([]const u8), bind: contract.protocol.ExtraBind) !void {
    try dup(alloc, list, bind.mode.bwrapFlag());
    try dup(alloc, list, bind.path);
    try dup(alloc, list, bind.path);
}

/// Append the bubblewrap wrapper: namespaces + ro system + rw workspace + the
/// runner binary ro-bound (so the sandbox can exec it) + the per-lease resolver
/// files when egress is enabled + `--`. INTERIM (until 2.0.1 option D): the
/// opt-in `allow_all` posture re-shares the host netns (`--share-net`) so the
/// lease has full egress while filtered-veth enforcement is unbuilt;
/// `allow_list_egress` (strict / fail-closed default) keeps its own netns and
/// `deny_all` stays fully unshared (no network).
fn appendBwrap(io: std.Io, alloc: std.mem.Allocator, list: *std.ArrayList([]const u8), self_exe: []const u8, workspace: []const u8, egress: ?EgressFiles, net_policy: Policy.Mode, extra_binds: []const contract.protocol.ExtraBind) !void {
    const bwrap = bwrapPath(io) orelse return error.BwrapUnavailable;
    try sandbox_bind_guard.assertBindTargetsSafe(io, extra_binds);
    return appendBwrapAt(alloc, list, bwrap, self_exe, workspace, egress, net_policy, extra_binds);
}

/// The wrapper's composition, given an ALREADY-RESOLVED bwrap path. Split from
/// `appendBwrap` so the argv contract is provable without a bubblewrap binary
/// on the box: this arm touches no filesystem and reads no `builtin.os.tag`.
///
/// That split is not a convenience. Before it, every bind-contract test gated
/// on `error.BwrapUnavailable` and skipped — on a Mac because the host is not
/// Linux, and in continuous integration because the CI image ships no
/// bubblewrap (the product `Dockerfile` installs it; the CI image does not). So
/// the tests guarding which paths reach a lease ran NOWHERE, which is how a
/// missing `/run/systemd/resolve` shipped and broke every lease for a week.
/// Composition is a pure function of the policy, so it is tested as one.
fn appendBwrapAt(alloc: std.mem.Allocator, list: *std.ArrayList([]const u8), bwrap: []const u8, self_exe: []const u8, workspace: []const u8, egress: ?EgressFiles, net_policy: Policy.Mode, extra_binds: []const contract.protocol.ExtraBind) !void {
    // `--new-session` detaches the controlling terminal (no TIOCSTI input
    // injection if a tty is ever attached); it sits with the other namespace
    // flags so every sandboxed tier gets it.
    // The executable and library trees arrive through `RO_SYSTEM_PATHS`, not
    // from this base argv: the runner is static, but the engine's transport
    // spawns `curl`. Sourcing them there keeps landlock derived from bwrap.
    const base = [_][]const u8{
        bwrap,           "--die-with-parent", "--unshare-all",
        "--new-session", "--proc",            "/proc",
        "--dev",         "/dev",
    };
    for (base) |a| try dup(alloc, list, a);
    // One private tmpfs per writable-floor entry, from the shared list landlock
    // also consumes — mount layer and policy layer cannot disagree on writes.
    for (contract.protocol.BASELINE_RW_TMPFS) |p|
        for ([_][]const u8{ "--tmpfs", p }) |a| try dup(alloc, list, a);
    // The child's HOME, created on the floor above. `--dir` rather than trusting
    // the engine to make it: its credential path calls `makePath` (which creates
    // parents) but its config path calls `makeDirAbsolute` (which does not), so a
    // missing parent fails one caller and not the other. Ordered after the tmpfs
    // mounts because the directory has to land ON the tmpfs, not under it.
    for ([_][]const u8{ "--dir", contract.protocol.CHILD_HOME }) |a| try dup(alloc, list, a);
    // The resolver, as a LINK into the granted resolver directory. Binding it
    // instead makes bwrap resolve the symlink and drop the target file into an
    // `/etc` landlock does not cover — measured on a real host as
    // `resolver=0 dns=0 egress=0`, every lease losing DNS.
    for ([_][]const u8{
        "--symlink",
        contract.protocol.RESOLV_LINK_TARGET,
        contract.protocol.RESOLV_LINK,
    }) |a| try dup(alloc, list, a);
    // Baseline then operator additions, composed by the pure helper so the
    // additive-only invariant is asserted independently of this platform arm.
    // `extra_binds` is validated (`extraBindsValid`) before it reaches the
    // holder; an invalid list never gets this far.
    var bind_buf: [MAX_BINDS]contract.protocol.ExtraBind = undefined;
    for (composeBinds(&bind_buf, extra_binds)) |b| try bindTry(alloc, list, b);
    try dup(alloc, list, "--bind");
    try dup(alloc, list, workspace);
    try dup(alloc, list, workspace);
    try dup(alloc, list, RO_BIND);
    try dup(alloc, list, self_exe);
    try dup(alloc, list, self_exe);
    try dup(alloc, list, "--chdir");
    try dup(alloc, list, workspace);
    // The opt-in `allow_all` posture re-shares the host netns so the lease
    // has full egress; `allow_list_egress` (the fail-closed default) keeps an
    // unshared netns (egress arrives via the EgressScope veth) and `deny_all`
    // has no network. Driven by the Policy strategy, not a hardcoded compare.
    if (net_policy.sharesHostNet()) try dup(alloc, list, SHARE_NET);
    // Resolver files: bind the parent-rendered static hosts + neutered
    // resolv.conf over the child's, so allowlist names resolve via /etc/hosts
    // and no resolver is reachable (port 53 is dropped at nft). Bound only when
    // EgressScope established them — the net namespace stays unshared regardless.
    if (egress) |e| {
        try dup(alloc, list, RO_BIND);
        try dup(alloc, list, e.hosts_path);
        try dup(alloc, list, ETC_HOSTS);
        try dup(alloc, list, RO_BIND);
        try dup(alloc, list, e.resolv_path);
        try dup(alloc, list, ETC_RESOLV);
    }
    try dup(alloc, list, "--");
}

/// First present bubblewrap binary, or null. Pub for the capability probe.
pub fn bwrapPath(io: std.Io) ?[]const u8 {
    for (BWRAP_PATHS) |p| {
        std.Io.Dir.accessAbsolute(io, p, .{}) catch continue;
        return p;
    }
    return null;
}
