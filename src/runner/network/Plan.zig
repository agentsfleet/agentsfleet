//! Plan.zig — the deterministic per-lease egress plan.
//!
//! Pure derivation from `(worker_index, resolved allowlist entries)`: the veth
//! pair's interface names + point-to-point `/30` addresses, and the two files
//! bound into the sandbox — a static `/etc/hosts` (allowlist names → their
//! resolved IPs) and a neutered `/etc/resolv.conf`. File-as-struct (`@This()`),
//! no I/O — `EgressScope` resolves names → `entries` (DNS) and consumes this.
//!
//! Launch slice is IPv4: `hostsFile` renders A-record entries; a v6 entry is a
//! resolution-filter bug here and fails closed (`UnsupportedAddressFamily`).
//! v6 in the static hosts file is a follow-up.

const Plan = @This();

/// One resolved allowlist hop: a hostname and the address it resolved to at
/// lease setup. Borrowed by `Plan` — the caller (`EgressScope`) owns the slice.
pub const HostEntry = struct {
    name: []const u8,
    addr: std.Io.net.IpAddress,
};

pub const Error = error{ WorkerIndexOutOfRange, UnsupportedAddressFamily, OutOfMemory };

worker_index: u32,
/// Host-side veth interface name, e.g. `uzveth0` (owned).
host_ifname: []const u8,
/// Child-side veth peer name at creation, e.g. `uzveth0p` (owned).
child_ifname: []const u8,
host_addr: std.Io.net.IpAddress, // 10.69.<idx>.1
child_addr: std.Io.net.IpAddress, // 10.69.<idx>.2
prefix_len: u8,
/// Resolved allowlist (name → IP). Borrowed; not freed by `deinit`.
entries: []const HostEntry,
alloc: std.mem.Allocator,

// Point-to-point veth subnet 10.69.<idx>.0/30 (RFC1918, /30 = host .1, child .2,
// network .0, broadcast .3). Single-sourced (RULE UFS).
const VETH_OCTET_A: u8 = 10;
const VETH_OCTET_B: u8 = 69;
const HOST_OCTET: u8 = 1;
const CHILD_OCTET: u8 = 2;
const PREFIX_LEN: u8 = 30;
const IFNAME_PREFIX = "uzveth";
// Third octet holds the worker index, so the pool cannot exceed one octet.
const MAX_WORKER_INDEX: u32 = 253;

/// Derive the plan. `entries` is the already-resolved allowlist (borrowed).
/// Allocates the two interface names; free with `deinit`.
pub fn build(alloc: std.mem.Allocator, worker_index: u32, entries: []const HostEntry) Error!Plan {
    if (worker_index > MAX_WORKER_INDEX) return error.WorkerIndexOutOfRange;
    const idx: u8 = @intCast(worker_index);

    const host_ifname = try std.fmt.allocPrint(alloc, "{s}{d}", .{ IFNAME_PREFIX, worker_index });
    errdefer alloc.free(host_ifname);
    const child_ifname = try std.fmt.allocPrint(alloc, "{s}{d}p", .{ IFNAME_PREFIX, worker_index });
    errdefer alloc.free(child_ifname);

    return .{
        .worker_index = worker_index,
        .host_ifname = host_ifname,
        .child_ifname = child_ifname,
        .host_addr = ip4(VETH_OCTET_A, VETH_OCTET_B, idx, HOST_OCTET),
        .child_addr = ip4(VETH_OCTET_A, VETH_OCTET_B, idx, CHILD_OCTET),
        .prefix_len = PREFIX_LEN,
        .entries = entries,
        .alloc = alloc,
    };
}

/// Render the per-lease `/etc/hosts`: one `IP name` line per allowlist entry.
/// Caller owns the returned slice. Fails closed on a non-IPv4 entry.
pub fn hostsFile(self: Plan, alloc: std.mem.Allocator) Error![]u8 {
    var buf: std.ArrayList(u8) = .empty;
    errdefer buf.deinit(alloc);
    for (self.entries) |e| try appendHostsLine(alloc, &buf, e);
    return buf.toOwnedSlice(alloc);
}

/// The per-lease `/etc/resolv.conf`: deliberately resolver-less. Allowlisted
/// names resolve via the static `/etc/hosts`; with port 53 dropped at nft, any
/// DNS attempt fails fast and the DNS-tunnel channel stays closed. Static — no
/// allocation.
pub fn resolvConf() []const u8 {
    return "# usezombie egress: names resolve via /etc/hosts only; no resolver.\n";
}

pub fn deinit(self: *Plan) void {
    self.alloc.free(self.host_ifname);
    self.alloc.free(self.child_ifname);
    self.host_ifname = "";
    self.child_ifname = "";
}

fn ip4(a: u8, b: u8, c: u8, d: u8) std.Io.net.IpAddress {
    return .{ .ip4 = .{ .bytes = .{ a, b, c, d }, .port = 0 } };
}

fn appendHostsLine(alloc: std.mem.Allocator, buf: *std.ArrayList(u8), e: HostEntry) Error!void {
    const v4 = switch (e.addr) {
        .ip4 => |x| x,
        .ip6 => return error.UnsupportedAddressFamily,
    };
    const line = try std.fmt.allocPrint(alloc, "{d}.{d}.{d}.{d} {s}\n", .{
        v4.bytes[0], v4.bytes[1], v4.bytes[2], v4.bytes[3], e.name,
    });
    defer alloc.free(line);
    try buf.appendSlice(alloc, line);
}

// ── Tests ───────────────────────────────────────────────────────────────────

fn entry(name: []const u8, a: u8, b: u8, c: u8, d: u8) HostEntry {
    return .{ .name = name, .addr = ip4(a, b, c, d) };
}

test "build derives veth ifnames and /30 addresses from worker_index" {
    const al = std.testing.allocator;
    var p = try build(al, 0, &.{});
    defer p.deinit();
    try std.testing.expectEqualStrings("uzveth0", p.host_ifname);
    try std.testing.expectEqualStrings("uzveth0p", p.child_ifname);
    try std.testing.expectEqual([4]u8{ 10, 69, 0, 1 }, p.host_addr.ip4.bytes);
    try std.testing.expectEqual([4]u8{ 10, 69, 0, 2 }, p.child_addr.ip4.bytes);
    try std.testing.expectEqual(@as(u8, 30), p.prefix_len);
}

test "build gives each worker a distinct third octet" {
    const al = std.testing.allocator;
    var p7 = try build(al, 7, &.{});
    defer p7.deinit();
    try std.testing.expectEqualStrings("uzveth7", p7.host_ifname);
    try std.testing.expectEqual([4]u8{ 10, 69, 7, 1 }, p7.host_addr.ip4.bytes);
}

test "build rejects a worker_index past one octet (fail closed)" {
    const al = std.testing.allocator;
    try std.testing.expectError(error.WorkerIndexOutOfRange, build(al, 254, &.{}));
}

test "hostsFile renders one IP-name line per entry, in order" {
    const al = std.testing.allocator;
    const entries = [_]HostEntry{
        entry("api.fireworks.ai", 1, 2, 3, 4),
        entry("registry.npmjs.org", 10, 20, 30, 40),
    };
    var p = try build(al, 0, &entries);
    defer p.deinit();
    const hosts = try p.hostsFile(al);
    defer al.free(hosts);
    try std.testing.expectEqualStrings(
        "1.2.3.4 api.fireworks.ai\n10.20.30.40 registry.npmjs.org\n",
        hosts,
    );
}

test "hostsFile on an empty allowlist is empty" {
    const al = std.testing.allocator;
    var p = try build(al, 0, &.{});
    defer p.deinit();
    const hosts = try p.hostsFile(al);
    defer al.free(hosts);
    try std.testing.expectEqual(@as(usize, 0), hosts.len);
}

test "hostsFile fails closed on a non-IPv4 entry (launch slice is v4)" {
    const al = std.testing.allocator;
    const entries = [_]HostEntry{
        .{ .name = "v6.example", .addr = try std.Io.net.IpAddress.parseIp6("::1", 0) },
    };
    var p = try build(al, 0, &entries);
    defer p.deinit();
    try std.testing.expectError(error.UnsupportedAddressFamily, p.hostsFile(al));
}

test "resolvConf is resolver-less (no nameserver directive)" {
    const rc = resolvConf();
    try std.testing.expect(std.mem.indexOf(u8, rc, "nameserver") == null);
    try std.testing.expect(rc.len > 0);
}

const std = @import("std");
