//! egress_integration_test.zig — Linux behaviour proofs for the per-lease
//! egress datapath (EgressScope: veth + host-side nftables + child netns).
//!
//! Unlike the pure golden-byte tests (which prove our netlink bytes match real
//! `nft`), these prove the bytes, once SENT, make the kernel do the right
//! thing. They are deliberately BINARY-FREE — no `nft`/`ip` subprocess — both
//! because the runner itself shells out to neither (it speaks netlink direct,
//! so the test should too) and because `create()` already carries its own
//! kernel-acceptance proof: `Socket.roundTrip` requires a positive ACK per
//! command and fails closed on `NLMSG_ERROR`, so a `create()` that returns
//! without error means the kernel accepted every table/chain/set/rule.
//!
//! What is proven here:
//!   - create() succeeds → the kernel ACKed the full ruleset transaction;
//!   - the host-side veth exists after create, gone after destroy;
//!   - attachChild moves the peer interface into the CHILD's netns (the child
//!     self-verifies via /proc/net/dev inside its own namespace);
//!   - destroy is idempotent (a second call no-ops, no double-free).
//!
//! What is deferred (needs a dummy-iface + ip_forward + listener rig): the
//! packet-level allow/deny CONTRAST — an allowlisted forwarded dest connects
//! while a non-allowlisted one is dropped. In a hermetic container a bare
//! "denied connect fails" is inconclusive (no route ≠ nft drop), so that proof
//! lands with the rig rather than as a false-confidence assertion here.
//!
//! Gated on Linux + root (CAP_NET_ADMIN); SkipZigTest otherwise. Runs on the
//! `test-integration-runner` lane. Worker index is fixed per test so the
//! veth/table names are deterministic; each test destroys its scope.

const std = @import("std");
const builtin = @import("builtin");
const linux = std.os.linux;

const Plan = @import("Plan.zig");
const EgressScope = @import("EgressScope.zig");

// A worker index unlikely to collide with anything else on the box.
const TEST_WORKER: u32 = 200;

/// Skip unless we can actually program the kernel: Linux + effective root.
fn requireLinuxRoot() !void {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    if (linux.geteuid() != 0) return error.SkipZigTest;
}

/// True iff interface `ifname` is present in the CURRENT network namespace.
/// Reads `/proc/net/dev` — which is namespace-aware (`/proc/net` → the task's
/// own netns) — NOT `/sys/class/net`, which keeps showing the original netns
/// until sysfs is remounted (the trap: a freshly-unshared child would still
/// see the host's interfaces via /sys). Raw syscalls — safe post-fork.
fn linkInNetns(ifname: []const u8) bool {
    const fd_raw = linux.openat(linux.AT.FDCWD, "/proc/net/dev", .{}, 0);
    if (linux.errno(fd_raw) != .SUCCESS) return false;
    const fd: i32 = @intCast(fd_raw);
    defer _ = linux.close(fd);

    var buf: [8192]u8 = undefined;
    var total: usize = 0;
    while (total < buf.len) {
        const n = linux.read(fd, buf[total..].ptr, buf.len - total);
        if (linux.errno(n) != .SUCCESS) return false;
        if (n == 0) break;
        total += n;
    }
    // Interfaces appear as "<ifname>:" (the colon disambiguates uzveth200 from
    // uzveth200p). Build the needle on the stack.
    var needle_buf: [32]u8 = undefined;
    const needle = std.fmt.bufPrint(&needle_buf, "{s}:", .{ifname}) catch return false;
    return std.mem.indexOf(u8, buf[0..total], needle) != null;
}

fn ip4(a: u8, b: u8, c: u8, d: u8) std.Io.net.IpAddress {
    return .{ .ip4 = .{ .bytes = .{ a, b, c, d }, .port = 0 } };
}

test "create ACKs the full ruleset + veth; destroy removes it; idempotent" {
    try requireLinuxRoot();
    const alloc = std.testing.allocator;

    // One literal allowlist IP — no DNS in the datapath proof.
    const entries = [_]Plan.HostEntry{.{ .name = "allowed.test", .addr = ip4(10, 200, 0, 1) }};
    var plan = try Plan.build(alloc, TEST_WORKER, &entries);
    defer plan.deinit();

    // create() returning ok IS the proof the kernel accepted the table, both
    // chains, the set + element, and all four rules (roundTrip ACK-checks each).
    var scope = EgressScope.create(alloc, &plan) catch |err| {
        std.debug.print("egress create failed: {s}\n", .{@errorName(err)});
        return err;
    };
    defer scope.destroy();

    // The host-side veth now exists.
    try std.testing.expect(linkInNetns(plan.host_ifname));

    scope.destroy(); // explicit first teardown
    try std.testing.expect(!linkInNetns(plan.host_ifname)); // veth gone

    // The deferred destroy fires again below → must be a safe no-op.
}

test "attachChild moves the peer interface into the child's netns" {
    try requireLinuxRoot();
    const alloc = std.testing.allocator;

    const entries = [_]Plan.HostEntry{.{ .name = "allowed.test", .addr = ip4(10, 200, 0, 1) }};
    var plan = try Plan.build(alloc, TEST_WORKER, &entries);
    defer plan.deinit();

    var scope = try EgressScope.create(alloc, &plan);
    defer scope.destroy();

    // ready: child → parent ("netns created"); barrier: parent → child ("veth ready").
    var ready: [2]i32 = undefined;
    var barrier: [2]i32 = undefined;
    var result: [2]i32 = undefined; // child → parent: 1 verdict byte
    if (linux.pipe2(&ready, .{}) != 0) return error.PipeFailed;
    if (linux.pipe2(&barrier, .{}) != 0) return error.PipeFailed;
    if (linux.pipe2(&result, .{}) != 0) return error.PipeFailed;

    const pid_raw = linux.fork();
    const pid: i32 = @intCast(@as(isize, @bitCast(pid_raw)));
    if (pid < 0) return error.ForkFailed;
    if (pid == 0) childProbe(plan.child_ifname, ready, barrier, result); // never returns

    _ = linux.close(ready[1]);
    _ = linux.close(barrier[0]);
    _ = linux.close(result[1]);

    // Wait for the child to report its fresh netns is created. Before attach,
    // the peer does NOT exist in the child netns — the child confirms that too.
    var rb: [1]u8 = undefined;
    _ = linux.read(ready[0], &rb, 1);
    try std.testing.expectEqual(@as(u8, 'n'), rb[0]); // 'n' = netns made, peer absent

    // Move the veth peer into the child's netns + configure it.
    try scope.attachChild(pid);

    // Release the child to re-check: the peer interface must now be present.
    _ = linux.write(barrier[1], "go", 2);

    var verdict: [1]u8 = undefined;
    const n = linux.read(result[0], &verdict, 1);
    var status: u32 = 0;
    _ = linux.waitpid(pid, &status, 0);

    try std.testing.expect(n == 1);
    try std.testing.expectEqual(@as(u8, 'Y'), verdict[0]); // 'Y' = peer present in child netns
}

/// Forked child: unshare a new net namespace, confirm the peer is ABSENT
/// (sends 'n'), block until the parent attaches the veth, then confirm the
/// peer is PRESENT (sends 'Y') — proving attachChild crossed the namespace
/// boundary. Raw syscalls only — runs post-fork, pre-exec.
fn childProbe(peer_ifname: []const u8, ready: [2]i32, barrier: [2]i32, result: [2]i32) noreturn {
    _ = linux.close(ready[0]);
    _ = linux.close(barrier[1]);
    _ = linux.close(result[0]);

    if (linux.unshare(linux.CLONE.NEWNET) != 0) childExit(result[1], 'E');
    // In the fresh netns the peer cannot exist yet.
    const before: u8 = if (linkInNetns(peer_ifname)) 'P' else 'n';
    _ = linux.write(ready[1], &[_]u8{before}, 1);

    var b: [2]u8 = undefined;
    _ = linux.read(barrier[0], &b, 2); // block until the parent moves the veth in

    const after: u8 = if (linkInNetns(peer_ifname)) 'Y' else 'N';
    childExit(result[1], after);
}

fn childExit(result_w: i32, verdict: u8) noreturn {
    _ = linux.write(result_w, &[_]u8{verdict}, 1);
    linux.exit(0);
}

test {
    std.testing.refAllDecls(@This());
}
