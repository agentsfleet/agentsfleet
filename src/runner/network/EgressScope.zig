//! EgressScope.zig — per-lease egress lifecycle (mirrors `engine/cgroup.zig`'s
//! CgroupScope): `create` builds the host side — veth pair, host /30 address,
//! link up, and the full default-deny nftables ruleset in the HOST netns on
//! the host-side veth (root-owned; never inside the child's netns, which the
//! child could flush) — `attachChild` moves the peer into the child's netns
//! and configures it from a scratch thread, `destroy` tears everything down
//! idempotently. Fail-closed by construction: with no rules installed the
//! child's netns has no egress at all, and a v6 allowlist entry refuses setup
//! (the launch slice is IPv4; the inet-family drop policy disposes of v6
//! packets). Callers: `attachChild` only after the child's net namespace
//! exists (post-unshare) — moving the peer earlier is a no-op into the host
//! ns and the child ends up with no egress (closed, but broken).

const EgressScope = @This();

plan: *const Plan,
destroyed: bool = false,

pub const Error = error{
    UnsupportedPlatform,
    SetupFailed,
    AttachFailed,
    UnsupportedAddressFamily,
    OutOfMemory,
} || Socket.Error || MessageBuilder.Error;

// Single source for the nftables object names (tests + capture.sh mirror these).
// The TABLE is PER-WORKER (`uz_egress<idx>`): each worker owns its own table so
// one worker's teardown (`destroy` → `delTable`) drops only its own chains/
// rules, never another concurrent worker's (RUNNER_WORKER_COUNT > 1). Chains are
// scoped within the per-worker table, so their names can stay constant.
pub const TABLE_PREFIX = "uz_egress";
pub const CHAIN_FWD = "egress_fwd";
pub const CHAIN_NAT = "egress_nat";
pub const SET_PREFIX = "allow";
/// `<prefix><worker_index>` format for the per-worker set + table names.
const PREFIXED_NAME_FMT = "{s}{d}";
const PRIO_FILTER: i32 = 0;
const PRIO_SRCNAT: i32 = 100;
const SET_ID: u32 = 1; // ties NEWSETELEM to NEWSET within one transaction
const BATCH_BUF_LEN = 4096;
const MSG_BUF_LEN = 512;
const LO_IFINDEX: i32 = 1;
const IFNAMSIZ = 16;
const SIOCGIFINDEX: u32 = 0x8933;

/// `allow<worker_index>` into `buf`. Plan caps worker_index at one octet, so
/// IFNAMSIZ can never overflow — overflowing is a programmer bug.
pub fn setName(buf: []u8, worker_index: u32) []const u8 {
    return std.fmt.bufPrint(buf, PREFIXED_NAME_FMT, .{ SET_PREFIX, worker_index }) catch
        @panic("egress set name exceeds IFNAMSIZ");
}

/// `uz_egress<worker_index>` into `buf` — the per-worker nftables table name.
/// Per-worker so a worker's `destroy` deletes only its own table.
pub fn tableName(buf: []u8, worker_index: u32) []const u8 {
    return std.fmt.bufPrint(buf, PREFIXED_NAME_FMT, .{ TABLE_PREFIX, worker_index }) catch
        @panic("egress table name exceeds buffer");
}

/// Host-side setup: veth pair + host address/up + the nftables ruleset.
/// On any failure the partially-created veth is torn down (fail closed).
pub fn create(alloc: std.mem.Allocator, plan: *const Plan) Error!EgressScope {
    if (builtin.os.tag != .linux) return error.UnsupportedPlatform;

    const addrs = try collectV4(alloc, plan.entries);
    defer alloc.free(addrs);

    var rt = try Socket.open(.route);
    defer rt.close();

    var buf: [MSG_BUF_LEN]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    rtnetlink.newVethPair(&mb, plan.host_ifname, plan.child_ifname, 1);
    try rt.roundTrip(buf[0..(try mb.finish()).len]);
    errdefer deleteVeth(plan.host_ifname);

    const host_if = try ifIndex(plan.host_ifname);
    mb = MessageBuilder.init(&buf);
    rtnetlink.newAddr(&mb, host_if, ip4Bytes(plan.host_addr), plan.prefix_len, 2);
    try rt.roundTrip(buf[0..(try mb.finish()).len]);
    mb = MessageBuilder.init(&buf);
    rtnetlink.setLinkUp(&mb, @intCast(host_if), 3);
    try rt.roundTrip(buf[0..(try mb.finish()).len]);

    try installRuleset(plan, addrs);
    log.info("egress_created", .{ .host_if = plan.host_ifname, .allow_count = addrs.len });
    return .{ .plan = plan };
}

/// Move the veth peer into `pid`'s netns and configure it (address, links up,
/// default route via the host hop) from a scratch thread that enters the
/// namespace and exits — the worker thread never changes namespace.
pub fn attachChild(self: *const EgressScope, pid: std.posix.pid_t) Error!void {
    if (builtin.os.tag != .linux) return error.UnsupportedPlatform;

    const child_if = try ifIndex(self.plan.child_ifname);
    var rt = try Socket.open(.route);
    defer rt.close();
    var buf: [MSG_BUF_LEN]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    rtnetlink.moveLinkToNetns(&mb, @intCast(child_if), @intCast(pid), 1);
    try rt.roundTrip(buf[0..(try mb.finish()).len]);

    var setup = ChildNetnsSetup{ .plan = self.plan, .pid = pid };
    const t = std.Thread.spawn(.{}, ChildNetnsSetup.run, .{&setup}) catch return error.AttachFailed;
    t.join();
    try setup.result;
    log.info("egress_attached", .{ .pid = pid });
}

/// Idempotent teardown: drop the nft table (rules/sets/chains with it) and
/// the veth pair. Already-gone objects are fine — a child-netns exit has
/// usually destroyed the peer side first. Never fails the caller.
pub fn destroy(self: *EgressScope) void {
    if (builtin.os.tag != .linux or self.destroyed) return;
    self.destroyed = true;
    deleteTable(self.plan.worker_index) catch |err|
        log.debug("nft_teardown_skipped", .{ .err = @errorName(err) });
    deleteVeth(self.plan.host_ifname);
    log.info("egress_destroyed", .{ .host_if = self.plan.host_ifname });
}

/// Capability probe: can this host open the netlink sockets at all?
pub fn isAvailable() bool {
    if (builtin.os.tag != .linux) return false;
    var s = Socket.open(.netfilter) catch return false;
    s.close();
    return true;
}

// ── host-side helpers ────────────────────────────────────────────────────────

/// The full default-deny ruleset as ONE nftables transaction.
fn installRuleset(plan: *const Plan, addrs: []const [4]u8) Error!void {
    var nf = try Socket.open(.netfilter);
    defer nf.close();

    var name_buf: [IFNAMSIZ]u8 = undefined;
    const set = setName(&name_buf, plan.worker_index);
    var table_buf: [IFNAMSIZ]u8 = undefined;
    const table = tableName(&table_buf, plan.worker_index);
    var batch: [BATCH_BUF_LEN]u8 = undefined;
    var bw = BatchWriter{ .buf = &batch };

    nfnetlink.batchBegin(bw.next(), bw.seq);
    try bw.commit();
    nfnetlink.newTable(bw.next(), table, bw.seq);
    try bw.commit();
    nfnetlink.newChain(bw.next(), table, CHAIN_FWD, nfnetlink.CHAIN_TYPE_FILTER, nfnetlink.NF_INET_FORWARD, PRIO_FILTER, nfnetlink.NF_DROP, bw.seq);
    try bw.commit();
    nfnetlink.newChain(bw.next(), table, CHAIN_NAT, nfnetlink.CHAIN_TYPE_NAT, nfnetlink.NF_INET_POST_ROUTING, PRIO_SRCNAT, null, bw.seq);
    try bw.commit();
    nfnetlink.newSet(bw.next(), table, set, SET_ID, bw.seq);
    try bw.commit();
    nfnetlink.addSetElems(bw.next(), table, set, addrs, bw.seq);
    try bw.commit();
    nfnetlink_rule.newRuleDnsDrop(bw.next(), table, CHAIN_FWD, plan.host_ifname, .udp, bw.seq);
    try bw.commit();
    nfnetlink_rule.newRuleDnsDrop(bw.next(), table, CHAIN_FWD, plan.host_ifname, .tcp, bw.seq);
    try bw.commit();
    nfnetlink_rule.newRuleAllowSet(bw.next(), table, CHAIN_FWD, plan.host_ifname, set, SET_ID, bw.seq);
    try bw.commit();
    nfnetlink_rule.newRuleCtReturn(bw.next(), table, CHAIN_FWD, plan.host_ifname, bw.seq);
    try bw.commit();
    nfnetlink_rule.newRuleMasquerade(bw.next(), table, CHAIN_NAT, ip4Bytes(plan.host_addr), plan.prefix_len, plan.host_ifname, bw.seq);
    try bw.commit();
    nfnetlink.batchEnd(bw.next(), bw.seq);
    try bw.commit();

    try nf.roundTrip(batch[0..bw.len]);
}

/// Packs consecutive netlink messages into one buffer: `next()` hands a
/// builder over the remaining space (the message serializes in place, zero
/// copies), `commit()` advances the cursor + sequence number. Overflow
/// surfaces as BufferTooSmall at commit — fail closed, no truncated batch.
const BatchWriter = struct {
    buf: []u8,
    len: usize = 0,
    seq: u32 = 0,
    mb: ?MessageBuilder = null,

    fn next(self: *BatchWriter) *MessageBuilder {
        self.mb = MessageBuilder.init(self.buf[self.len..]);
        return &self.mb.?;
    }

    fn commit(self: *BatchWriter) MessageBuilder.Error!void {
        var mb = self.mb orelse return error.BufferTooSmall;
        self.len += (try mb.finish()).len;
        self.seq += 1;
        self.mb = null;
    }
};

fn ifIndex(name: []const u8) Error!u32 {
    const linux = std.os.linux;
    const Ifreq = extern struct { name: [IFNAMSIZ]u8, ivalue: i32, pad: [20]u8 };
    if (name.len >= IFNAMSIZ) return error.SetupFailed;
    var req = std.mem.zeroes(Ifreq);
    @memcpy(req.name[0..name.len], name);
    const rc = linux.socket(linux.AF.INET, linux.SOCK.DGRAM | linux.SOCK.CLOEXEC, 0);
    if (linux.errno(rc) != .SUCCESS) return error.SetupFailed;
    const fd: i32 = @intCast(rc);
    defer _ = linux.close(fd);
    if (linux.errno(linux.ioctl(fd, SIOCGIFINDEX, @intFromPtr(&req))) != .SUCCESS)
        return error.SetupFailed;
    return @intCast(req.ivalue);
}

fn deleteTable(worker_index: u32) Error!void {
    var nf = try Socket.open(.netfilter);
    defer nf.close();
    var table_buf: [IFNAMSIZ]u8 = undefined;
    const table = tableName(&table_buf, worker_index);
    var batch: [MSG_BUF_LEN]u8 = undefined;
    var blen: usize = 0;
    var mb = MessageBuilder.init(batch[blen..]);
    nfnetlink.batchBegin(&mb, 0);
    blen += (try mb.finish()).len;
    mb = MessageBuilder.init(batch[blen..]);
    nfnetlink.delTable(&mb, table, 1);
    blen += (try mb.finish()).len;
    mb = MessageBuilder.init(batch[blen..]);
    nfnetlink.batchEnd(&mb, 2);
    blen += (try mb.finish()).len;
    try nf.roundTrip(batch[0..blen]);
}

fn deleteVeth(host_ifname: []const u8) void {
    var rt = Socket.open(.route) catch return;
    defer rt.close();
    var buf: [MSG_BUF_LEN]u8 = undefined;
    var mb = MessageBuilder.init(&buf);
    rtnetlink.delLink(&mb, host_ifname, 1);
    const msg = mb.finish() catch return;
    rt.roundTrip(buf[0..msg.len]) catch |err|
        log.debug("veth_teardown_skipped", .{ .err = @errorName(err) });
}

/// IPv4 bytes of every allowlist entry; a v6 entry fails setup closed (the
/// launch slice is v4 — same refusal Plan.hostsFile makes).
fn collectV4(alloc: std.mem.Allocator, entries: []const Plan.HostEntry) Error![]const [4]u8 {
    const addrs = try alloc.alloc([4]u8, entries.len);
    errdefer alloc.free(addrs);
    for (entries, 0..) |e, i| {
        addrs[i] = switch (e.addr) {
            .ip4 => |x| x.bytes,
            .ip6 => return error.UnsupportedAddressFamily,
        };
    }
    return addrs;
}

fn ip4Bytes(addr: std.Io.net.IpAddress) [4]u8 {
    return switch (addr) {
        .ip4 => |x| x.bytes,
        .ip6 => unreachable, // Plan.build only constructs v4 veth addresses
    };
}

// ── child-netns configuration (scratch thread; setns dies with it) ──────────

const ChildNetnsSetup = struct {
    plan: *const Plan,
    pid: std.posix.pid_t,
    result: Error!void = error.AttachFailed,

    fn run(self: *ChildNetnsSetup) void {
        self.result = self.work();
    }

    fn work(self: *ChildNetnsSetup) Error!void {
        const linux = std.os.linux;
        var path_buf: [64]u8 = undefined;
        const path = std.fmt.bufPrintZ(&path_buf, "/proc/{d}/ns/net", .{self.pid}) catch
            return error.AttachFailed;
        const ns_rc = linux.openat(linux.AT.FDCWD, path, .{ .CLOEXEC = true }, 0);
        if (linux.errno(ns_rc) != .SUCCESS) return error.AttachFailed;
        const ns_fd: i32 = @intCast(ns_rc);
        defer _ = linux.close(ns_fd);
        if (linux.errno(linux.setns(ns_fd, linux.CLONE.NEWNET)) != .SUCCESS)
            return error.AttachFailed;

        var rt = try Socket.open(.route); // opened INSIDE the child netns
        defer rt.close();
        const child_if = try ifIndex(self.plan.child_ifname);
        var buf: [MSG_BUF_LEN]u8 = undefined;
        var mb = MessageBuilder.init(&buf);
        rtnetlink.newAddr(&mb, child_if, ip4Bytes(self.plan.child_addr), self.plan.prefix_len, 1);
        try rt.roundTrip(buf[0..(try mb.finish()).len]);
        mb = MessageBuilder.init(&buf);
        rtnetlink.setLinkUp(&mb, @intCast(child_if), 2);
        try rt.roundTrip(buf[0..(try mb.finish()).len]);
        mb = MessageBuilder.init(&buf);
        rtnetlink.setLinkUp(&mb, LO_IFINDEX, 3);
        try rt.roundTrip(buf[0..(try mb.finish()).len]);
        mb = MessageBuilder.init(&buf);
        rtnetlink.newDefaultRoute(&mb, child_if, ip4Bytes(self.plan.host_addr), 4);
        try rt.roundTrip(buf[0..(try mb.finish()).len]);
    }
};

// ── Tests (pure halves; behavior is the Linux integration lane) ─────────────

test "create is fail-closed off Linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    var p = try Plan.build(std.testing.allocator, 0, &.{});
    defer p.deinit();
    try std.testing.expectError(error.UnsupportedPlatform, create(std.testing.allocator, &p));
}

test "setName derives the per-worker set name" {
    var buf: [IFNAMSIZ]u8 = undefined;
    try std.testing.expectEqualStrings("allow0", setName(&buf, 0));
    try std.testing.expectEqualStrings("allow253", setName(&buf, 253));
}

test "collectV4 gathers v4 bytes and fails closed on a v6 entry" {
    const al = std.testing.allocator;
    const v4 = [_]Plan.HostEntry{
        .{ .name = "a", .addr = .{ .ip4 = .{ .bytes = .{ 1, 2, 3, 4 }, .port = 0 } } },
    };
    const got = try collectV4(al, &v4);
    defer al.free(got);
    try std.testing.expectEqual([4]u8{ 1, 2, 3, 4 }, got[0]);

    const v6 = [_]Plan.HostEntry{
        .{ .name = "b", .addr = try std.Io.net.IpAddress.parseIp6("::1", 0) },
    };
    try std.testing.expectError(error.UnsupportedAddressFamily, collectV4(al, &v6));
}

const Plan = @import("Plan.zig");
const Socket = @import("Socket.zig");
const MessageBuilder = @import("MessageBuilder.zig");
const rtnetlink = @import("rtnetlink.zig");
const nfnetlink = @import("nfnetlink.zig");
const nfnetlink_rule = @import("nfnetlink_rule.zig");
const std = @import("std");
const builtin = @import("builtin");
const log = @import("log").scoped(.egress_scope);
