//! selftest_probe.zig — the `__selftest_probe` child arm of the runner.
//!
//! Runs INSIDE the sandbox, under the assigned policy, spawned through the same
//! `buildSandboxPrefix` a lease gets. Its whole job is to answer three questions
//! a host-side check cannot: does the resolver file resolve here, does a name
//! resolve here, and does a declared endpoint accept a connection here.
//!
//! Why an arm of the runner binary rather than `getent`/`curl`: the sandbox
//! carries whatever the HOST carries, and the host is not ours. The product
//! image is `debian:bookworm-slim` with no `curl`; a baremetal runner is
//! whatever the operator installed. Shelling out to a tool that is absent
//! reports a broken sandbox on a healthy host — the false red this milestone
//! exists to remove. The runner's own binary is already `--ro-bind`-ed into
//! every sandbox (`sandbox_args.appendBwrapAt`), so probing through it adds no
//! mount, no dependency, and no new exposure.
//!
//! Dispatched ahead of the operator subcommand registry in `main.zig`, exactly
//! as `__execute` is, so it never reaches `--help`: the operator command
//! surface stays unchanged and `doctor` remains the one host-side entry point.
//!
//! The child prints ONE fixed-shape line and exits 0. It never prints what it
//! saw — no address, no hostname, no errno text — because the parent turns this
//! line into an operator-visible verdict and a probe that echoed its input
//! would be a path for a hostname or an environment value to reach stored
//! output (Invariant 7).

const std = @import("std");
const contract = @import("contract");
const child_exec = @import("child_exec.zig");
const sandbox_hardening = @import("sandbox_hardening.zig");

/// argv subcommand selecting probe mode. Deliberately `__`-prefixed like
/// `__execute`: dispatched before the operator registry, absent from help.
pub const SUBCOMMAND = "__selftest_probe";

/// Exit for a probe whose sandbox hardening could not be established. Non-zero
/// so the parent grades it as an unestablished sandbox, never as a verdict —
/// the same fail-closed posture `__execute` takes.
const HARDENING_FAIL_EXIT: u8 = 1;

/// The name to resolve, supplied by the parent from the ASSIGNED registry
/// allowlist. Absent → the DNS check reports untested rather than inventing a
/// target: probing a name the operator never declared red-flags a runner that
/// is configured exactly as intended.
pub const RESOLVE_FLAG_PREFIX = "--resolve=";

/// `host:port` to dial, likewise drawn from the assignment. Absent → untested.
pub const DIAL_FLAG_PREFIX = "--dial=";

// Operator binds arrive on `child_exec`'s mode-explicit flags — the SAME wire a
// lease child reads (RULE UFS: one vocabulary) — repeatable up to
// `MAX_EXTRA_BINDS`. Each is confirmed landed (Dimension 4.5: without the
// check, the operator's only signal that an entry never landed is a tenant's
// failed run) AND admitted into the probe's own landlock ruleset at its mode,
// exactly as a lease admits it.

/// The file whose dangling symlink was the M167 incident: on a systemd host it
/// links into `/run/systemd/resolve`, which the sandbox did not bind, so every
/// lookup inside every lease failed for a week while `doctor` said `ok: true`.
pub const RESOLV_PATH = "/etc/resolv.conf";

/// Lookup queue capacity. `HostName.lookup` is documented as non-blocking at
/// >= 16, and it closes the queue before returning, so the drain below
/// terminates without a timeout of its own.
const LOOKUP_CAPACITY = 16;

/// One check's observed state, encoded as the single character the parent
/// parses. `untested` is deliberately distinct from `failed`: "we did not ask"
/// and "we asked and it broke" are different facts, and collapsing them reports
/// an undeclared target as a dead resolver.
pub const Verdict = enum(u8) {
    failed = '0',
    passed = '1',
    untested = 'x',

    fn of(ok: bool) Verdict {
        return if (ok) .passed else .failed;
    }
};

/// The one line the child writes. Keys are fixed and ordered; the parent
/// matches on them rather than on position, so an added check cannot silently
/// shift an existing verdict.
pub const KEY_RESOLVER = "resolver=";
pub const KEY_DNS = "dns=";
pub const KEY_EGRESS = "egress=";
pub const KEY_BINDS = "binds=";

/// Run the probe. Always exits 0 on a completed run — a FAILED CHECK is a
/// result, not an error. A non-zero exit means the child could not run at all,
/// which the parent reports as an unestablished sandbox instead of a verdict.
pub fn run(argv: []const [:0]const u8, io: std.Io) u8 {
    var resolve_host: ?[]const u8 = null;
    var dial_target: ?[]const u8 = null;
    var sandboxed = false;
    var workspace: ?[]const u8 = null;
    for (argv[2..]) |a| {
        if (std.mem.startsWith(u8, a, RESOLVE_FLAG_PREFIX))
            resolve_host = a[RESOLVE_FLAG_PREFIX.len..];
        if (std.mem.startsWith(u8, a, DIAL_FLAG_PREFIX))
            dial_target = a[DIAL_FLAG_PREFIX.len..];
        if (std.mem.eql(u8, a, child_exec.SANDBOXED_FLAG)) sandboxed = true;
        if (std.mem.startsWith(u8, a, child_exec.WORKSPACE_FLAG_PREFIX))
            workspace = a[child_exec.WORKSPACE_FLAG_PREFIX.len..];
    }
    var bind_buf: [contract.protocol.MAX_EXTRA_BINDS]contract.protocol.ExtraBind = undefined;
    const extra_binds = sandbox_hardening.collectBindFlags(argv, &bind_buf) catch return HARDENING_FAIL_EXIT;

    // The lease child's exact hardening, BEFORE any check runs — a probe
    // outside the landlock/seccomp wall reported the M136 resolver fault as
    // healthy, because only the constrained child could not read the resolver
    // target. Failure is an unestablished sandbox, not a verdict.
    if (sandboxed) {
        const ws = workspace orelse return HARDENING_FAIL_EXIT;
        sandbox_hardening.applySandboxHardening(ws, extra_binds) catch return HARDENING_FAIL_EXIT;
    }

    var binds_present = true;
    for (extra_binds) |b| {
        if (!pathPresent(io, b.path)) binds_present = false;
    }
    const binds_seen = extra_binds.len;

    const resolver = Verdict.of(resolverResolves(io));
    const dns = if (resolve_host) |h| Verdict.of(nameResolves(io, h)) else .untested;
    const egress = if (dial_target) |t| Verdict.of(endpointAccepts(io, t)) else .untested;
    const binds: Verdict = if (binds_seen == 0) .untested else Verdict.of(binds_present);

    writeVerdict(io, resolver, dns, egress, binds);
    return 0;
}

/// Did an operator-assigned bind actually land in this sandbox? Directory or
/// file — `accessAbsolute` answers for both. The MODE is not re-checked here
/// because bwrap enforces it: a read-only bind is read-only in the kernel, not
/// by our assertion.
fn pathPresent(io: std.Io, path: []const u8) bool {
    std.Io.Dir.accessAbsolute(io, path, .{}) catch return false;
    return true;
}

/// Does `/etc/resolv.conf` resolve to something openable from in here?
///
/// Open success is the whole check, deliberately — the question is whether the
/// path RESOLVES, not what it says. A dangling symlink (the incident) fails to
/// open; an intentionally neutered resolv.conf, which `EgressScope` binds under
/// a locked-down posture, opens and is empty. Grading on content would call
/// that correct configuration a fault.
fn resolverResolves(io: std.Io) bool {
    const file = std.Io.Dir.openFileAbsolute(io, RESOLV_PATH, .{}) catch return false;
    file.close(io);
    return true;
}

/// Does a name resolve from in here? Any returned address is a pass; the
/// address itself is discarded without being formatted, so it cannot reach the
/// output line.
fn nameResolves(io: std.Io, host: []const u8) bool {
    const name = std.Io.net.HostName.init(host) catch return false;
    var buf: [LOOKUP_CAPACITY]std.Io.net.HostName.LookupResult = undefined;
    var queue: std.Io.Queue(std.Io.net.HostName.LookupResult) = .init(&buf);
    name.lookup(io, &queue, .{ .port = 0 }) catch return false;

    var resolved = false;
    while (queue.getOneUncancelable(io)) |item| {
        switch (item) {
            .address => resolved = true,
            .canonical_name => {},
        }
    } else |err| switch (err) {
        // `lookup` closes the queue before it returns; Closed is the normal
        // end of the drain, not a failure.
        error.Closed => {},
    }
    return resolved;
}

/// Does a declared `host:port` accept a connection from in here? The stream is
/// closed the instant it opens — this proves reachability, it does not speak a
/// protocol.
fn endpointAccepts(io: std.Io, target: []const u8) bool {
    const split = std.mem.lastIndexOfScalar(u8, target, ':') orelse return false;
    const port = std.fmt.parseInt(u16, target[split + 1 ..], 10) catch return false;
    const name = std.Io.net.HostName.init(target[0..split]) catch return false;
    const stream = name.connect(io, port, .{ .mode = .stream }) catch return false;
    stream.close(io);
    return true;
}

/// Emit the verdict line on stdout, on a stack buffer — the probe allocates
/// nothing, so it can still report on a host where the fault IS memory.
///
/// Every failure here is swallowed with a log line rather than propagated: a
/// closed stdout means the parent already reaped us, and there is no one left
/// to tell. The parent reads a missing line as every check failing, which is
/// the fail-closed reading.
fn writeVerdict(io: std.Io, resolver: Verdict, dns: Verdict, egress: Verdict, binds: Verdict) void {
    var out_buf: [64]u8 = undefined;
    var stdout_w = std.Io.File.stdout().writer(io, &out_buf);
    const stdout = &stdout_w.interface;
    stdout.print("{s}{c} {s}{c} {s}{c} {s}{c}\n", .{
        KEY_RESOLVER, @intFromEnum(resolver),
        KEY_DNS,      @intFromEnum(dns),
        KEY_EGRESS,   @intFromEnum(egress),
        KEY_BINDS,    @intFromEnum(binds),
    }) catch |err| {
        std.log.warn("selftest probe verdict write ignored: {s}", .{@errorName(err)});
        return;
    };
    stdout.flush() catch |err|
        std.log.warn("selftest probe verdict flush ignored: {s}", .{@errorName(err)});
}

test {
    _ = @import("selftest_probe_test.zig");
}
