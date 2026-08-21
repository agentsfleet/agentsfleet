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
const selftest_transport = @import("selftest_transport.zig");

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

/// Absolute path of the binary the ENGINE's model transport spawns, resolved by
/// the parent on the host. Absent → untested, never invented: a probe that
/// guessed a path would report on a transport this host does not have.
///
/// This check exists because the egress check above cannot answer the question.
/// `endpointAccepts` opens a TCP stream from inside the statically linked
/// runner and spawns nothing, so it measured the one path in a lease that needs
/// no executable — and M170 §3 removed the executable trees on the strength of
/// it, which would have killed every lease at `execvp` before its first model
/// call. Reachability and executability are different facts; this key is the
/// second one.
pub const TRANSPORT_FLAG_PREFIX = "--transport=";

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
pub const KEY_SCRATCH = "scratch=";
pub const KEY_HOME = "home=";
pub const KEY_DEV_FILES = "devfiles=";
pub const KEY_TRANSPORT = "transport=";

/// Run the probe. Always exits 0 on a completed run — a FAILED CHECK is a
/// result, not an error. A non-zero exit means the child could not run at all,
/// which the parent reports as an unestablished sandbox instead of a verdict.
pub fn run(argv: []const [:0]const u8, env_map: *const std.process.Environ.Map, io: std.Io) u8 {
    var resolve_host: ?[]const u8 = null;
    var dial_target: ?[]const u8 = null;
    var transport_path: ?[]const u8 = null;
    var sandboxed = false;
    var workspace: ?[]const u8 = null;
    for (argv[2..]) |a| {
        if (std.mem.startsWith(u8, a, RESOLVE_FLAG_PREFIX))
            resolve_host = a[RESOLVE_FLAG_PREFIX.len..];
        if (std.mem.startsWith(u8, a, DIAL_FLAG_PREFIX))
            dial_target = a[DIAL_FLAG_PREFIX.len..];
        if (std.mem.startsWith(u8, a, TRANSPORT_FLAG_PREFIX))
            transport_path = a[TRANSPORT_FLAG_PREFIX.len..];
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
    const scratch = Verdict.of(scratchWritable(io));
    const home = Verdict.of(homeWritable(io, env_map));
    const dev_files = Verdict.of(deviceFilesWritable(io));
    const dns = if (resolve_host) |h| Verdict.of(nameResolves(io, h)) else .untested;
    const egress = if (dial_target) |t| Verdict.of(endpointAccepts(io, t)) else .untested;
    const transport = if (transport_path) |p| Verdict.of(selftest_transport.execs(p)) else .untested;
    const binds: Verdict = if (binds_seen == 0) .untested else Verdict.of(binds_present);

    writeVerdict(io, resolver, scratch, home, dev_files, dns, egress, transport, binds);
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

/// Can this constrained child create a file in its scratch tmpfs? The engine
/// writes credentialed dial headers there, so a floor entry the policy layer
/// refuses fails every lease at its first credentialed call
/// (TempFileCreateFailed) — exactly the fault the write floor exists to
/// prevent, detected here rather than assumed from the lists agreeing.
fn scratchWritable(io: std.Io) bool {
    inline for (contract.protocol.BASELINE_RW_TMPFS) |dir| {
        // Unique per run: under dev_none the probe runs on the HOST /tmp, so a
        // fixed name could collide with a concurrent probe's file or a crashed
        // predecessor's leftover. Exclusive: proves MAKE_REG precisely (an
        // existing file cannot stand in for a create), and O_EXCL refuses to
        // follow a planted symlink outside the private tmpfs.
        var path_buf: [dir.len + 64]u8 = undefined;
        const path = std.fmt.bufPrint(&path_buf, "{s}/agentsfleet_selftest_scratch_{d}", .{ dir, std.c.getpid() }) catch return false;
        const f = std.Io.Dir.createFileAbsolute(io, path, .{ .exclusive = true }) catch return false;
        f.close(io);
        // Removal is part of the check: the floor grants REMOVE_FILE too, and
        // a scratch that fills with undeletable probe files is its own fault.
        std.Io.Dir.deleteFileAbsolute(io, path) catch return false;
    }
    return true;
}

/// Is this a `HOME` worth attempting a write under? Pure over the value, so the
/// three ways a home is unusable before any I/O happens are unit-testable
/// without a sandbox — the same decider/probe split `preflight` uses.
///
/// Absent is the case that shipped: the child had no `HOME` the sandbox could
/// reach, and grading that as anything but a failure is what let `all_ok=true`
/// stand on a host where every lease died.
pub fn homePathUsable(home: ?[]const u8) bool {
    const h = home orelse return false;
    if (h.len == 0) return false;
    // Relative is unusable, not merely odd: the probe resolves it against its
    // own cwd, which is the workspace, so a write would land somewhere the
    // engine never looks and report a home that works when it does not.
    if (!std.fs.path.isAbsolute(h)) return false;
    return true;
}

/// Can this constrained child write under the HOME it was actually given?
///
/// Distinct from `scratchWritable`, and the distinction is the whole point.
/// That check walks `BASELINE_RW_TMPFS` and proves the FLOOR is writable; it
/// says nothing about whether the child's `$HOME` is on that floor. It was
/// passing — `all_ok=true`, four checks green — on a host where every lease
/// died, because HOME pointed at `/run/agentsfleet` and no list carried it.
/// Reading the variable the child was handed is what turns that class of fault
/// from invisible into a failed check.
fn homeWritable(io: std.Io, env_map: *const std.process.Environ.Map) bool {
    // Read from the process map rather than a libc getenv: same source the lease
    // child's own env is built from, so the probe grades the value a lease sees.
    const home = env_map.get("HOME");
    if (!homePathUsable(home)) return false;
    const h = home.?;
    // Same exclusive create/remove as the scratch check: O_EXCL proves MAKE_REG
    // precisely and refuses to follow a planted symlink out of the sandbox.
    var path_buf: [std.fs.max_path_bytes]u8 = undefined;
    const path = std.fmt.bufPrint(&path_buf, "{s}/agentsfleet_selftest_home_{d}", .{ h, std.c.getpid() }) catch return false;
    const f = std.Io.Dir.createFileAbsolute(io, path, .{ .exclusive = true }) catch return false;
    f.close(io);
    std.Io.Dir.deleteFileAbsolute(io, path) catch return false;
    return true;
}

/// Can this constrained child open the policy layer's writable device files for
/// WRITING? `/dev/null` today, walked from the one list `applyPolicy` grants
/// from, so an entry added there is graded here without a second edit.
///
/// The narrowest check in this probe, and it exists because the widest one
/// missed it. `CHECK_TRANSPORT` proves the sandbox can EXECUTE the binary the
/// engine spawns; it never proves the spawn can wire that binary's stdio. The
/// engine's transport does exactly that — `open("/dev/null", O_RDWR)` on the way
/// to `curl` — and on a host whose policy layer had `/dev` read-only while
/// bwrap's `--dev` had it writable, every lease died there at zero tokens with
/// six checks green.
///
/// Read-write is the whole check: opening read-only would pass under the exact
/// mask that produced the incident, which is a check that cannot fail when it
/// matters. Nothing is written through the handle — `/dev/null` accepts anything
/// and reports nothing back, so a write proves less than the open already does.
///
/// The open mode is stated, never defaulted: `.read_write` is the whole check,
/// and a call that let the mode default would be measuring something the engine
/// never does.
fn deviceFilesWritable(io: std.Io) bool {
    inline for (sandbox_hardening.FLOOR_RW_FILES) |path| {
        const f = std.Io.Dir.openFileAbsolute(io, path, .{ .mode = .read_write }) catch return false;
        f.close(io);
    }
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
fn writeVerdict(io: std.Io, resolver: Verdict, scratch: Verdict, home: Verdict, dev_files: Verdict, dns: Verdict, egress: Verdict, transport: Verdict, binds: Verdict) void {
    // 128 holds the eight keys with room to spare (the line is ~74 bytes); the
    // parent's own read cap is `selftest_exec.VERDICT_READ_CAP` = 160, and a
    // ninth key would need both raised together.
    var out_buf: [128]u8 = undefined;
    var stdout_w = std.Io.File.stdout().writer(io, &out_buf);
    const stdout = &stdout_w.interface;
    stdout.print("{s}{c} {s}{c} {s}{c} {s}{c} {s}{c} {s}{c} {s}{c} {s}{c}\n", .{
        KEY_RESOLVER,  @intFromEnum(resolver),
        KEY_SCRATCH,   @intFromEnum(scratch),
        KEY_HOME,      @intFromEnum(home),
        KEY_DEV_FILES, @intFromEnum(dev_files),
        KEY_DNS,       @intFromEnum(dns),
        KEY_EGRESS,    @intFromEnum(egress),
        KEY_TRANSPORT, @intFromEnum(transport),
        KEY_BINDS,     @intFromEnum(binds),
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
