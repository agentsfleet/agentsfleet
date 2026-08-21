//! selftest.zig — prove a host can actually run work, from INSIDE a sandbox.
//!
//! The M167 incident: `/etc/resolv.conf` symlinks into `/run/systemd/resolve`,
//! which the sandbox never bound, so the symlink dangled inside every lease and
//! all DNS failed — for a week — while `doctor` reported healthy. `doctor`
//! probes the HOST. A host check cannot see a broken sandbox, so this runs the
//! checks where the work runs.
//!
//! Three properties keep it honest:
//!   1. The sandbox comes from `sandbox_args.buildSandboxPrefix` — the same
//!      construction a lease gets, not a parallel one (Invariant 1).
//!   2. Every detail is drawn from a fixed vocabulary, never from child output,
//!      so a check can never echo a token or an environment value (Invariant 7).
//!   3. A refused/absent mechanism is reported as a failed CHECK, not an error:
//!      the runner is never silently marked healthy.
//!
//! Verdict vocabulary is `doctor.Check{name, ok, detail}`, reused deliberately —
//! one runner must not report health two ways.

const std = @import("std");
const common = @import("common");
const contract = @import("contract");

const Config = @import("daemon/config.zig");
const child_exec = @import("child_exec.zig");
const sandbox_hardening = @import("sandbox_hardening.zig");
const doctor = @import("cmd/doctor.zig");
const sandbox_args = @import("sandbox_args.zig");
const selftest_probe = @import("selftest_probe.zig");
const selftest_transport = @import("selftest_transport.zig");

pub const Check = doctor.Check;

/// How long a probe child may run before it is reaped. A probe is a handful of
/// filesystem/DNS operations; anything past this is a hung resolver, which is
/// itself the fault we are looking for. Deliberately not a count — the count
/// grows as checks are added, and a stale number here reads as a budget
/// derivation that no longer holds.
///
/// It MUST stay well under the heartbeat interval. The probe runs on the
/// heartbeat path, so a probe that burns its whole bound delays the NEXT beat
/// by that much — at parity with the interval a single timing-out probe costs a
/// full beat, eating the margin `HEARTBEAT_INTERVAL_MS` keeps against
/// `RUNNER_OFFLINE_AFTER_MS`. A self-test that reported a host offline would be
/// worse than the fault it looks for.
pub const PROBE_TIMEOUT_MS: u64 = 5_000;

comptime {
    if (PROBE_TIMEOUT_MS * 2 > @as(u64, @intCast(common.HEARTBEAT_INTERVAL_MS)))
        @compileError("PROBE_TIMEOUT_MS must leave at least half the heartbeat interval, or a timing-out probe delays the beat that carries its verdict");
}

/// Check names. Operator-facing and stable — they appear on the runner page and
/// in the stored result, so a historical result stays readable after a rename
/// (RULE UFS: the dashboard and the daemon spell them the same way).
pub const CHECK_RESOLVER = "resolver file resolves inside the sandbox";
pub const CHECK_SCRATCH = "the scratch dir accepts a write inside the sandbox";
pub const CHECK_HOME = "the child's home accepts a write inside the sandbox";
pub const CHECK_DEV_FILES = "the writable device files open for writing inside the sandbox";
pub const CHECK_DNS = "a hostname resolves inside the sandbox";
pub const CHECK_EGRESS = "the inference endpoint is reachable";
pub const CHECK_TRANSPORT = "the model transport runs inside the sandbox";
pub const CHECK_SANDBOX = "a sandbox can be established";

/// Every `detail` a check may carry. A fixed vocabulary is what makes
/// Invariant 7 provable: the probe never interpolates child output, a hostname,
/// or an environment value into a result, so no result can carry a secret.
/// Prose, never empty — the dashboard reads a whitespace-free cause as a leaked
/// internal identifier and hides it, losing the check's explanation.
pub const DETAIL_OK = "no fault detected";
pub const DETAIL_SCRATCH_READONLY = "the sandbox refused a write to its scratch tmpfs — every credentialed dial fails as TempFileCreateFailed until the write floor is granted";
pub const DETAIL_HOME_UNREACHABLE = "the sandbox refused a write under the child's HOME — the engine cannot create its configuration directory, and every lease fails as AccessDenied before its first model call";
pub const DETAIL_DEV_FILES_READONLY = "the sandbox refused an open-for-write on /dev/null — the engine wires its model transport's stdio through it, so every lease fails as AccessDenied before its first model call";
pub const DETAIL_RESOLVER_DANGLING = "/etc/resolv.conf does not resolve to a readable file — the systemd-resolved stub is not bound into the sandbox";
pub const DETAIL_DNS_FAILED = "the resolver did not answer inside the sandbox";
pub const DETAIL_EGRESS_BLOCKED = "the endpoint did not accept a connection";
pub const DETAIL_EGRESS_DENIED_EXPECTED = "no egress by assignment (deny_all_egress) — expected, not a fault";
pub const DETAIL_TRANSPORT_UNEXECUTABLE = "the sandbox could not execute the model transport — the engine spawns curl for every model call, so every lease dies at execvp before its first one";
pub const DETAIL_TRANSPORT_ABSENT = "no curl binary at /usr/bin/curl or /bin/curl on this host — the engine spawns one for every model call, so no lease can reach a model";
pub const DETAIL_TIMEOUT = "the probe exceeded its time bound and was reaped";
pub const DETAIL_NO_BWRAP = "no bubblewrap binary on this host — a sandboxed tier cannot be established";
/// An assigned bind resolves onto a path the sandbox protects. Named for the
/// operator who assigned it: the spelling they entered is not the thing that
/// got refused, so the message says what the check was actually about.
pub const DETAIL_UNSAFE_BIND = "an assigned bind resolves onto a protected host path — leases are refused until it is removed";
pub const DETAIL_SPAWN_FAILED = "the sandbox could not be established";

/// Under `deny_all_egress` no name can resolve, by assignment. Graded the same
/// way the egress check is: reporting it a fault would make every correctly
/// locked-down runner read unhealthy, and an alert that fires on correct
/// configuration gets muted — then it is not there when the sandbox breaks.
pub const DETAIL_DNS_NO_NETWORK = "no egress by assignment (deny_all_egress), so no name can resolve — expected, not a fault";

/// Nothing asked the probe to resolve a name — the assignment declares no
/// registry, so there is no name the operator has said this runner needs.
/// Distinct from `DETAIL_DNS_FAILED`: "not tested" and "tested and broken" are
/// different facts, and collapsing them reports an undeclared target as a dead
/// resolver.
///
/// This no longer means "no tool in the sandbox". The probe performs the lookup
/// through the runner's own binary (`selftest_probe`), which is bound into
/// every sandbox, so the tool is never the missing piece — only a target is.
pub const DETAIL_DNS_NOT_TESTABLE = "no hostname is assigned to resolve, so name resolution was not tested";

/// No registry host is assigned, so the runner has declared no egress
/// requirement to prove. The probe never dials a host the operator did not
/// name — probing the daemon's own fallback guess would produce a red row
/// nobody can act on.
pub const DETAIL_EGRESS_NONE_DECLARED = "no registry hosts are assigned, so there is no declared egress requirement to test";

/// Per-bind details. The probe checks that an operator-assigned path RESOLVES
/// inside the sandbox; it does not re-test the mode, because bwrap enforces
/// that in the kernel. These say exactly that, rather than echoing the mode
/// label alone — a bare "read-only" as a check's detail reads as "verified
/// read-only", which is a claim the probe has not earned.
pub const DETAIL_BIND_PRESENT_RO = "mounted inside the sandbox; assigned read-only, a mode the kernel enforces rather than the probe";
pub const DETAIL_BIND_PRESENT_RW = "mounted inside the sandbox; assigned read-write, which widens the isolation boundary for every lease this runner takes";
pub const DETAIL_BIND_ABSENT = "not present inside the sandbox — this bind did not land on this host, so leases run without it";

/// `allow_list_egress` refuses the lease outright until option D lands
/// (`child_supervisor.enforcesEgress` fail-closed), so there is no sandbox to
/// probe. Reported as the same refusal a lease gets rather than as a verdict
/// on a sandbox that was never built.
pub const DETAIL_POSTURE_UNBUILDABLE = "the assigned egress posture is not established for leases on this build, so no sandbox was probed";

/// One probe run's verdict: the ordered checks, the policy they ran under, and
/// whether the sandbox was even established. The policy travels WITH the result
/// so a result that outlives its assignment renders as stale rather than as a
/// verdict on the current one (Invariant 4).
pub const Result = struct {
    checks: []const Check,
    network_policy: contract.protocol.NetworkPolicy,
    sandbox_tier: contract.protocol.SandboxTier,

    /// Every check passed. A probe that could not start has an `ok == false`
    /// check, so this is false there too — an unestablished sandbox is never
    /// mistaken for a healthy one.
    pub fn allOk(self: Result) bool {
        for (self.checks) |c| {
            if (!c.ok) return false;
        }
        return true;
    }

    pub fn deinit(self: Result, alloc: std.mem.Allocator) void {
        alloc.free(self.checks);
    }
};

/// What the probe is asked to reach, derived from the ASSIGNMENT rather than
/// from a default. Both are optional: a runner that declared no registry has no
/// egress requirement to prove, and inventing a target would red-flag a host
/// configured exactly as intended.
pub const ProbeTargets = struct {
    /// Host to resolve (no port).
    resolve: ?[]const u8 = null,
    /// `host:port` to dial.
    dial: ?[]const u8 = null,
    /// Operator-assigned binds to confirm landed. Borrowed from the config —
    /// the argv builder copies each path before the caller's config can go.
    binds: []const contract.protocol.ExtraBind = &.{},
    /// Absolute path of the engine's model transport, resolved on the HOST.
    ///
    /// Set by `buildProbeArgv`, which has an `Io` to look with; `targetsFor` is
    /// pure and leaves it null, so the pure argv twin composes a probe that
    /// reports the transport untested rather than one that guesses a path.
    transport: ?[]const u8 = null,
};

/// The first transport binary present on this host, or null. Re-exported from
/// `selftest_transport` so the parent and the probe resolve the same list
/// (RULE UFS) and callers keep one entry point.
pub const transportPath = selftest_transport.hostPath;

/// Port assumed when a declared registry names a bare host. Registries are
/// pulled over Transport Layer Security (TLS), so 443 is the reachability
/// question worth asking; `registryAllowlistValid` already permits `host:port`
/// for anything else.
pub const DEFAULT_REGISTRY_PORT = "443";

/// The probe's targets for one assignment: the FIRST declared registry, or
/// nothing. First rather than all because the checks answer "can this sandbox
/// reach the network at all" — one declared host settles that, and dialling
/// every entry would turn one operator click into N connections per runner.
///
/// Under `deny_all_egress` both stay null: `grade` reports those two checks as
/// expected-by-assignment without consulting a probe, so asking the child to
/// dial would spend a timeout to produce a verdict already known.
pub fn targetsFor(cfg: Config) ProbeTargets {
    // Binds are confirmed under EVERY posture: an operator-added mount is a
    // filesystem question, and a locked-down network says nothing about whether
    // the path landed.
    if (cfg.network_policy == .deny_all_egress) return .{ .binds = cfg.extra_binds };
    // No registry declared: resolve the daemon's own control-plane host —
    // resolve, never dial, so no egress requirement is invented. Without this
    // a default assignment graded DNS "not tested" and `all_ok` overstated on
    // exactly the host whose sandbox could not resolve anything.
    const first = if (cfg.registry_allowlist.len > 0)
        cfg.registry_allowlist[0]
    else
        return .{ .resolve = controlPlaneHost(cfg.control_plane_url), .binds = cfg.extra_binds };
    const colon = std.mem.lastIndexOfScalar(u8, first, ':');
    return .{
        .resolve = if (colon) |c| first[0..c] else first,
        .dial = first,
        .binds = cfg.extra_binds,
    };
}

/// The host component of the daemon's own control-plane URL, or null when it
/// does not parse — then the DNS check stays untested rather than probing a
/// name nobody configured.
fn controlPlaneHost(url: []const u8) ?[]const u8 {
    const uri = std.Uri.parse(url) catch return null;
    const host = uri.host orelse return null;
    return switch (host) {
        .raw => |s| s,
        .percent_encoded => |s| s,
    };
}

/// Build the probe's full argv: the lease sandbox prefix plus the probe
/// command. Free with `sandbox_args.freeArgv`.
pub fn buildProbeArgv(io: std.Io, alloc: std.mem.Allocator, cfg: Config, workspace_path: []const u8) ![]const []const u8 {
    const prefix = try sandbox_args.buildSandboxPrefix(io, alloc, cfg, workspace_path, null);
    defer sandbox_args.freeArgv(alloc, prefix);
    const self_exe = try sandbox_args.resolveChildExe(io, alloc);
    defer alloc.free(self_exe);
    var targets = targetsFor(cfg);
    // Resolved here rather than in `targetsFor` because it is a HOST fact, and
    // `targetsFor` is pure so the argv twin stays testable without a filesystem.
    targets.transport = transportPath(io);
    return appendProbeCommand(alloc, prefix, self_exe, workspace_path, targets);
}

/// The probe argv for a GIVEN bwrap binary and child exe — the pure twin of
/// `buildProbeArgv`, composed exactly the way a lease is. `pub` so Dimension
/// 2.1 can assert the two prefixes match on any platform: gating that
/// assertion on a real bubblewrap binary skipped it everywhere, which would
/// leave "the probe runs through the lease builder" asserted by nothing.
pub fn composeProbeArgv(alloc: std.mem.Allocator, bwrap: []const u8, self_exe: []const u8, cfg: Config, workspace_path: []const u8) ![]const []const u8 {
    const prefix = try sandbox_args.composeSandboxPrefix(alloc, bwrap, self_exe, cfg, workspace_path, null);
    defer sandbox_args.freeArgv(alloc, prefix);
    return appendProbeCommand(alloc, prefix, self_exe, workspace_path, targetsFor(cfg));
}

/// Copy `prefix` and append the probe's child command, transferring ownership
/// of the result to the caller.
///
/// The tail is the runner's own binary in `__selftest_probe` mode — the same
/// `self_exe` the prefix already `--ro-bind`s, so no mount is added for it.
/// Deliberately NOT `__execute`: a self-test must never run the real executor
/// (Invariant 1). Deliberately not a host tool either — see `selftest_probe`'s
/// header for why `curl`/`getent` cannot be assumed present.
fn appendProbeCommand(alloc: std.mem.Allocator, prefix: []const []const u8, self_exe: []const u8, workspace_path: []const u8, targets: ProbeTargets) ![]const []const u8 {
    var list: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (list.items) |s| alloc.free(s);
        list.deinit(alloc);
    }
    for (prefix) |s| try appendCopy(alloc, &list, s);
    try appendCopy(alloc, &list, self_exe);
    try appendCopy(alloc, &list, selftest_probe.SUBCOMMAND);
    // A non-empty prefix means a sandboxed tier: the probe then applies the
    // lease child's exact in-child hardening (no_new_privs → landlock →
    // seccomp), so its verdicts hold under the SAME constraints a lease runs
    // under — the flags are `child_exec`'s own (RULE UFS: one wire).
    if (prefix.len > 0) {
        try appendCopy(alloc, &list, child_exec.SANDBOXED_FLAG);
        try appendFlag(alloc, &list, child_exec.WORKSPACE_FLAG_PREFIX, workspace_path);
    }
    if (targets.resolve) |h| try appendFlag(alloc, &list, selftest_probe.RESOLVE_FLAG_PREFIX, h);
    if (targets.dial) |d| try appendDialFlag(alloc, &list, d);
    if (targets.transport) |t| try appendFlag(alloc, &list, selftest_probe.TRANSPORT_FLAG_PREFIX, t);
    for (targets.binds) |b| {
        const bind_prefix = switch (b.mode) {
            .read_only => sandbox_hardening.BIND_RO_FLAG_PREFIX,
            .read_write => sandbox_hardening.BIND_RW_FLAG_PREFIX,
        };
        try appendFlag(alloc, &list, bind_prefix, b.path);
    }
    return list.toOwnedSlice(alloc);
}

/// Append one owned copy of `s`. Split out so every append in
/// `appendProbeCommand` carries the same errdefer-safe ownership transfer
/// rather than repeating the three-line dance per argument.
fn appendCopy(alloc: std.mem.Allocator, list: *std.ArrayList([]const u8), s: []const u8) !void {
    const copy = try alloc.dupe(u8, s);
    errdefer alloc.free(copy);
    try list.append(alloc, copy);
}

/// Append `<prefix><value>` as one argv entry.
fn appendFlag(alloc: std.mem.Allocator, list: *std.ArrayList([]const u8), prefix: []const u8, value: []const u8) !void {
    const joined = try std.fmt.allocPrint(alloc, "{s}{s}", .{ prefix, value });
    errdefer alloc.free(joined);
    try list.append(alloc, joined);
}

/// Append the dial flag, defaulting a bare host to the registry port so the
/// child never has to guess one.
fn appendDialFlag(alloc: std.mem.Allocator, list: *std.ArrayList([]const u8), target: []const u8) !void {
    if (std.mem.lastIndexOfScalar(u8, target, ':') != null)
        return appendFlag(alloc, list, selftest_probe.DIAL_FLAG_PREFIX, target);
    const with_port = try std.fmt.allocPrint(alloc, "{s}:{s}", .{ target, DEFAULT_REGISTRY_PORT });
    defer alloc.free(with_port);
    return appendFlag(alloc, list, selftest_probe.DIAL_FLAG_PREFIX, with_port);
}

/// The verdict for a host whose sandbox could not be established at all. A
/// named failed check, never an error return: the caller records a result and
/// the operator reads WHY, rather than seeing an empty self-test.
pub fn unavailable(alloc: std.mem.Allocator, cfg: Config, detail: []const u8) !Result {
    const checks = try alloc.alloc(Check, 1);
    checks[0] = .{ .name = CHECK_SANDBOX, .ok = false, .detail = detail };
    return .{
        .checks = checks,
        .network_policy = cfg.network_policy,
        .sandbox_tier = cfg.sandbox_tier,
    };
}

/// Grade one probe run into the operator-facing check list.
///
/// `deny_all_egress` is the subtle arm: under that assignment there is no
/// network by design, so an unreachable endpoint is the CORRECT outcome and is
/// graded `ok` with a detail saying it was expected. Grading it a fault would
/// make every correctly-configured deny_all runner report unhealthy, and an
/// alert that fires on correct configuration gets muted — then it is not there
/// when the sandbox really breaks.
pub fn grade(alloc: std.mem.Allocator, cfg: Config, outcome: Outcome) !Result {
    var checks: std.ArrayList(Check) = .empty;
    errdefer checks.deinit(alloc);

    try checks.append(alloc, .{
        .name = CHECK_RESOLVER,
        .ok = outcome.resolver_readable,
        .detail = if (outcome.resolver_readable) DETAIL_OK else DETAIL_RESOLVER_DANGLING,
    });

    // Scratch is graded unconditionally: every posture's leases write their
    // credentialed dial headers there, so no assignment makes a refused write
    // expected. A timed-out probe observed nothing — say that, not "refused".
    try checks.append(alloc, .{
        .name = CHECK_SCRATCH,
        .ok = if (outcome.timed_out) false else outcome.scratch_writable,
        .detail = if (outcome.timed_out)
            DETAIL_TIMEOUT
        else if (outcome.scratch_writable)
            DETAIL_OK
        else
            DETAIL_SCRATCH_READONLY,
    });

    // Graded unconditionally for the same reason scratch is, and separately from
    // it because they answer different questions: scratch proves the writable
    // FLOOR exists, this proves the child's HOME is ON it. A runner that passes
    // the first and fails the second runs no lease at all — which is precisely
    // the state that reported `all_ok=true, checks=4` while every lease died.
    try checks.append(alloc, .{
        .name = CHECK_HOME,
        .ok = if (outcome.timed_out) false else outcome.home_writable,
        .detail = if (outcome.timed_out)
            DETAIL_TIMEOUT
        else if (outcome.home_writable)
            DETAIL_OK
        else
            DETAIL_HOME_UNREACHABLE,
    });

    // Graded unconditionally, and separately from the two writes above for the
    // same reason they are separate from each other: they answer different
    // questions. Scratch proves the writable FLOOR exists, home proves the
    // child's HOME sits on it, and this proves the one thing outside that floor
    // a lease must still be able to write. A runner passing the first two and
    // failing this one runs no lease at all — the state that reported
    // `all_ok=true, checks=6` while every lease died at `open("/dev/null",
    // O_RDWR)`.
    try checks.append(alloc, .{
        .name = CHECK_DEV_FILES,
        .ok = if (outcome.timed_out) false else outcome.device_files_writable,
        .detail = if (outcome.timed_out)
            DETAIL_TIMEOUT
        else if (outcome.device_files_writable)
            DETAIL_OK
        else
            DETAIL_DEV_FILES_READONLY,
    });

    // DNS is graded against the ASSIGNED posture too. Under deny_all there is
    // no network to resolve through, so a failure there is the assignment
    // working — the same reasoning the egress arm below has always used.
    if (outcome.timed_out) {
        try checks.append(alloc, .{ .name = CHECK_DNS, .ok = false, .detail = DETAIL_TIMEOUT });
    } else if (cfg.network_policy == .deny_all_egress) {
        try checks.append(alloc, .{ .name = CHECK_DNS, .ok = true, .detail = DETAIL_DNS_NO_NETWORK });
    } else if (!outcome.dns_testable) {
        try checks.append(alloc, .{ .name = CHECK_DNS, .ok = true, .detail = DETAIL_DNS_NOT_TESTABLE });
    } else {
        try checks.append(alloc, .{
            .name = CHECK_DNS,
            .ok = outcome.dns_resolved,
            .detail = if (outcome.dns_resolved) DETAIL_OK else DETAIL_DNS_FAILED,
        });
    }

    // Egress is graded against the ASSIGNED posture, not against an absolute.
    // The probe dials only hosts the operator DECLARED: an assigned registry is
    // an assertion that leases need it, so failing to reach one is a real fault
    // — whereas dialing the daemon's own fallback set would red-flag a runner
    // that is configured exactly as intended.
    if (cfg.network_policy == .deny_all_egress) {
        try checks.append(alloc, .{ .name = CHECK_EGRESS, .ok = true, .detail = DETAIL_EGRESS_DENIED_EXPECTED });
    } else if (cfg.registry_allowlist.len == 0) {
        try checks.append(alloc, .{ .name = CHECK_EGRESS, .ok = true, .detail = DETAIL_EGRESS_NONE_DECLARED });
    } else {
        try checks.append(alloc, .{
            .name = CHECK_EGRESS,
            .ok = outcome.egress_reachable,
            .detail = if (outcome.egress_reachable) DETAIL_OK else DETAIL_EGRESS_BLOCKED,
        });
    }

    // The transport is graded under EVERY posture, including `deny_all_egress`:
    // "can this sandbox execute the binary the engine spawns" is a filesystem
    // question, and a lease with no network still dies at `execvp` if the answer
    // is no. Absence is a fault rather than untested — a host with no transport
    // runs no lease, and reporting that as "nothing to measure" is exactly the
    // green-probe/dead-sandbox reading this milestone exists to remove.
    if (!outcome.transport_testable) {
        try checks.append(alloc, .{ .name = CHECK_TRANSPORT, .ok = false, .detail = DETAIL_TRANSPORT_ABSENT });
    } else {
        try checks.append(alloc, .{
            .name = CHECK_TRANSPORT,
            .ok = outcome.transport_execs,
            .detail = if (outcome.transport_execs) DETAIL_OK else DETAIL_TRANSPORT_UNEXECUTABLE,
        });
    }

    // One named check per operator-added bind, so an operator sees WHICH entry
    // did not land rather than one aggregate verdict (Dimension 4.5). The mode
    // travels with it — a writable mount is never reported silently.
    for (cfg.extra_binds) |b| {
        try checks.append(alloc, .{
            .name = b.path,
            .ok = outcome.extra_binds_present,
            .detail = bindDetail(b.mode, outcome.extra_binds_present),
        });
    }

    return .{
        .checks = try checks.toOwnedSlice(alloc),
        .network_policy = cfg.network_policy,
        .sandbox_tier = cfg.sandbox_tier,
    };
}

/// The fixed-vocabulary detail for one operator bind. A missing bind says so
/// plainly; a present one names the assigned mode without claiming to have
/// verified it.
fn bindDetail(mode: contract.protocol.BindMode, present: bool) []const u8 {
    if (!present) return DETAIL_BIND_ABSENT;
    return switch (mode) {
        .read_only => DETAIL_BIND_PRESENT_RO,
        .read_write => DETAIL_BIND_PRESENT_RW,
    };
}

/// What one probe run observed. Booleans only — the raw child output never
/// leaves the probe, which is what keeps a secret out of a stored result.
pub const Outcome = struct {
    resolver_readable: bool,
    /// No default on purpose: `grade` always reads it, so every construction
    /// site must decide — a silently-defaulted pass here is the exact false
    /// confidence the probe exists to remove.
    scratch_writable: bool,
    /// No default, for the same reason `scratch_writable` has none: `grade`
    /// always reads it, so every construction site must decide rather than
    /// inherit a pass it never observed.
    home_writable: bool,
    /// No default, for the reason the two above have none: `grade` always reads
    /// it, and a construction site that inherits a pass it never observed is
    /// how a policy layer that refused this open kept reporting a healthy host.
    device_files_writable: bool,
    dns_resolved: bool,
    egress_reachable: bool,
    extra_binds_present: bool = true,
    timed_out: bool = false,
    /// False when the sandbox carried no resolver tool to ask with. Defaults
    /// true so an existing caller keeps grading DNS as before; the executor
    /// sets it false rather than reporting an untested resolver as broken.
    dns_testable: bool = true,
    /// Did the transport the engine spawns actually execute inside the sandbox?
    /// No default, for the reason `scratch_writable` has none: `grade` always
    /// reads it, and a silently-defaulted pass here would re-create the exact
    /// unmeasured claim that removed the executable trees.
    transport_execs: bool,
    /// False when this host carries no transport binary at all, which `grade`
    /// reports as a fault distinct from one that failed to run — an operator
    /// fixes "install curl" and "fix the bind set" differently.
    transport_testable: bool,
};

test {
    _ = @import("selftest_test.zig");
}
