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
const contract = @import("contract");

const Config = @import("daemon/config.zig");
const doctor = @import("cmd/doctor.zig");
const sandbox_args = @import("sandbox_args.zig");

pub const Check = doctor.Check;

/// How long a probe child may run before it is reaped. A probe is three
/// filesystem/DNS operations; anything past this is a hung resolver, which is
/// itself the fault we are looking for.
pub const PROBE_TIMEOUT_MS: u64 = 10_000;

/// Check names. Operator-facing and stable — they appear on the runner page and
/// in the stored result, so a historical result stays readable after a rename
/// (RULE UFS: the dashboard and the daemon spell them the same way).
pub const CHECK_RESOLVER = "resolver file resolves inside the sandbox";
pub const CHECK_DNS = "a hostname resolves inside the sandbox";
pub const CHECK_EGRESS = "the inference endpoint is reachable";
pub const CHECK_SANDBOX = "a sandbox can be established";

/// Every `detail` a check may carry. A fixed vocabulary is what makes
/// Invariant 7 provable: the probe never interpolates child output, a hostname,
/// or an environment value into a result, so no result can carry a secret.
pub const DETAIL_OK = "";
pub const DETAIL_RESOLVER_DANGLING = "/etc/resolv.conf does not resolve to a readable file — the systemd-resolved stub is not bound into the sandbox";
pub const DETAIL_DNS_FAILED = "the resolver did not answer inside the sandbox";
pub const DETAIL_EGRESS_BLOCKED = "the endpoint did not accept a connection";
pub const DETAIL_EGRESS_DENIED_EXPECTED = "no egress by assignment (deny_all_egress) — expected, not a fault";
pub const DETAIL_TIMEOUT = "the probe exceeded its time bound and was reaped";
pub const DETAIL_NO_BWRAP = "no bubblewrap binary on this host — a sandboxed tier cannot be established";
pub const DETAIL_SPAWN_FAILED = "the sandbox could not be established";

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

/// The probe's child command, run INSIDE the sandbox. Deliberately not a new
/// `agentsfleet-runner` subcommand (the operator surface stays unchanged) and
/// deliberately not `__execute` — a self-test must not run the real executor.
///
/// `/bin/sh` and the files it reads are baseline binds, so this exercises the
/// exact mounts a lease depends on: reading `/etc/resolv.conf` fails when the
/// resolver stub is unbound, which is precisely the incident.
pub const PROBE_ARGV = [_][]const u8{ "/bin/sh", "-c", "cat /etc/resolv.conf" };

/// Build the probe's full argv: the lease sandbox prefix plus the probe
/// command. Free with `sandbox_args.freeArgv`.
pub fn buildProbeArgv(io: std.Io, alloc: std.mem.Allocator, cfg: Config, workspace_path: []const u8) ![]const []const u8 {
    const prefix = try sandbox_args.buildSandboxPrefix(io, alloc, cfg, workspace_path, null);
    defer sandbox_args.freeArgv(alloc, prefix);

    var list: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (list.items) |s| alloc.free(s);
        list.deinit(alloc);
    }
    for (prefix) |s| {
        const copy = try alloc.dupe(u8, s);
        errdefer alloc.free(copy);
        try list.append(alloc, copy);
    }
    for (PROBE_ARGV) |s| {
        const copy = try alloc.dupe(u8, s);
        errdefer alloc.free(copy);
        try list.append(alloc, copy);
    }
    return list.toOwnedSlice(alloc);
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

    if (outcome.timed_out) {
        try checks.append(alloc, .{ .name = CHECK_DNS, .ok = false, .detail = DETAIL_TIMEOUT });
    } else {
        try checks.append(alloc, .{
            .name = CHECK_DNS,
            .ok = outcome.dns_resolved,
            .detail = if (outcome.dns_resolved) DETAIL_OK else DETAIL_DNS_FAILED,
        });
    }

    // Egress is graded against the ASSIGNED posture, not against an absolute.
    if (cfg.network_policy == .deny_all_egress) {
        try checks.append(alloc, .{ .name = CHECK_EGRESS, .ok = true, .detail = DETAIL_EGRESS_DENIED_EXPECTED });
    } else {
        try checks.append(alloc, .{
            .name = CHECK_EGRESS,
            .ok = outcome.egress_reachable,
            .detail = if (outcome.egress_reachable) DETAIL_OK else DETAIL_EGRESS_BLOCKED,
        });
    }

    // One named check per operator-added bind, so an operator sees WHICH entry
    // did not land rather than one aggregate verdict (Dimension 4.5). The mode
    // travels with it — a writable mount is never reported silently.
    for (cfg.extra_binds) |b| {
        try checks.append(alloc, .{
            .name = b.path,
            .ok = outcome.extra_binds_present,
            .detail = b.mode.label(),
        });
    }

    return .{
        .checks = try checks.toOwnedSlice(alloc),
        .network_policy = cfg.network_policy,
        .sandbox_tier = cfg.sandbox_tier,
    };
}

/// What one probe run observed. Booleans only — the raw child output never
/// leaves the probe, which is what keeps a secret out of a stored result.
pub const Outcome = struct {
    resolver_readable: bool,
    dns_resolved: bool,
    egress_reachable: bool,
    extra_binds_present: bool = true,
    timed_out: bool = false,
};

test {
    _ = @import("selftest_test.zig");
}
