//! SSRF classification of IP-literal hosts — the range-checking half of the
//! base-URL guard, split out to keep each file under the RULE FLL line limit.
//!
//! Mirrors the runner's vendored `nullclaw/net_security.zig` isNonGlobalV4 /
//! isNonGlobalV6 predicates so the control-plane verdict (here) and the
//! data-plane enforcement (the runner, at connect time) agree on what counts as
//! SSRF-unsafe. Pure literal parsing — a hostname that is not an IP literal is
//! NOT classified here (DNS resolution safety is the runner's job downstream).

const std = @import("std");

// ── SSRF range constants (RULE UFS — shared verbatim with the tests) ─────────
// Named so a reviewer audits the blocklist at a glance and the tests pin the
// same numbers.

/// 127.0.0.0/8 — IPv4 loopback (first octet).
const V4_LOOPBACK_A: u8 = 127;
/// 10.0.0.0/8 — RFC1918 private (first octet).
const V4_PRIVATE_10_A: u8 = 10;
/// 172.16.0.0/12 — RFC1918 private (first octet + second-octet range).
const V4_PRIVATE_172_A: u8 = 172;
const V4_PRIVATE_172_B_LO: u8 = 16;
const V4_PRIVATE_172_B_HI: u8 = 31;
/// 192.168.0.0/16 — RFC1918 private (first + second octet).
const V4_PRIVATE_192_A: u8 = 192;
const V4_PRIVATE_192_B: u8 = 168;
/// 169.254.0.0/16 — link-local, incl. the cloud metadata host 169.254.169.254.
const V4_LINK_LOCAL_A: u8 = 169;
const V4_LINK_LOCAL_B: u8 = 254;
/// 0.0.0.0/8 — unspecified / "this host" (first octet).
const V4_UNSPECIFIED_A: u8 = 0;
/// 224.0.0.0/4 .. 255.255.255.255 — multicast through broadcast (first octet).
const V4_MULTICAST_MIN_A: u8 = 224;

/// IPv6 ff00::/8 multicast — high byte of the first segment.
const V6_MULTICAST_HIGH: u16 = 0xff00;
/// IPv6 fc00::/7 unique-local — first segment masked.
const V6_ULA_MASK: u16 = 0xfe00;
const V6_ULA_VALUE: u16 = 0xfc00;
/// IPv6 fe80::/10 link-local — first segment masked.
const V6_LINK_LOCAL_MASK: u16 = 0xffc0;
const V6_LINK_LOCAL_VALUE: u16 = 0xfe80;
/// IPv4-mapped prefix ::ffff:0:0/96 — segment[5] sentinel.
const V6_MAPPED_FFFF: u16 = 0xffff;

/// True when `host` is an IP literal in an SSRF-unsafe range. A hostname that is
/// not an IP literal passes (returns false) — DNS resolution safety is enforced
/// downstream by the runner, matching nullclaw's host-literal-only posture here.
/// `host` may be a bare host, a bracketed IPv6 literal, or carry an IPv6 zone id.
pub fn isBlockedHostLiteral(host: []const u8) bool {
    // Strip IPv6 brackets, then an optional zone id (`fe80::1%lo0`) — a zone id
    // never makes a blocked address global.
    const bare = if (host.len >= 2 and host[0] == '[' and host[host.len - 1] == ']')
        host[1 .. host.len - 1]
    else
        host;
    const unscoped = if (std.mem.indexOfScalar(u8, bare, '%')) |pct| bare[0..pct] else bare;
    if (unscoped.len == 0) return true;

    if (parseIpv4(unscoped)) |octets| return isBlockedV4(octets);
    if (parseIpv6(unscoped)) |segments| return isBlockedV6(segments);
    return false;
}

/// Mirror of nullclaw/net_security.zig isNonGlobalV4 (loopback/private/link-
/// local/unspecified/multicast-broadcast). Documentation/shared/benchmarking
/// ranges are intentionally omitted: they are globally *unroutable* but not an
/// SSRF target, and a tenant may legitimately front a real gateway elsewhere —
/// the spec blocklist is the security-relevant set.
fn isBlockedV4(addr: [4]u8) bool {
    const a = addr[0];
    const b = addr[1];
    if (a == V4_LOOPBACK_A) return true; // 127.0.0.0/8
    if (a == V4_PRIVATE_10_A) return true; // 10.0.0.0/8
    if (a == V4_PRIVATE_172_A and b >= V4_PRIVATE_172_B_LO and b <= V4_PRIVATE_172_B_HI) return true; // 172.16/12
    if (a == V4_PRIVATE_192_A and b == V4_PRIVATE_192_B) return true; // 192.168/16
    if (a == V4_LINK_LOCAL_A and b == V4_LINK_LOCAL_B) return true; // 169.254/16 (metadata)
    if (a == V4_UNSPECIFIED_A) return true; // 0.0.0.0/8
    if (a >= V4_MULTICAST_MIN_A) return true; // 224.0.0.0/4 .. broadcast
    return false;
}

/// Mirror of nullclaw/net_security.zig isNonGlobalV6 (loopback/unspecified/
/// multicast/unique-local/link-local + IPv4-mapped of any blocked v4).
fn isBlockedV6(segs: [8]u16) bool {
    if (isAllZeroExceptLast(segs, 1)) return true; // ::1 loopback
    if (isAllZeroExceptLast(segs, 0)) return true; // :: unspecified
    if (segs[0] & V6_MULTICAST_HIGH == V6_MULTICAST_HIGH) return true; // ff00::/8
    if (segs[0] & V6_ULA_MASK == V6_ULA_VALUE) return true; // fc00::/7
    if (segs[0] & V6_LINK_LOCAL_MASK == V6_LINK_LOCAL_VALUE) return true; // fe80::/10
    // ::ffff:a.b.c.d — IPv4-mapped: apply the v4 blocklist to the embedded addr.
    if (segs[0] == 0 and segs[1] == 0 and segs[2] == 0 and segs[3] == 0 and
        segs[4] == 0 and segs[5] == V6_MAPPED_FFFF)
    {
        return isBlockedV4(.{
            @truncate(segs[6] >> 8),
            @truncate(segs[6] & 0xff),
            @truncate(segs[7] >> 8),
            @truncate(segs[7] & 0xff),
        });
    }
    return false;
}

/// True when segments 0..6 are zero and segment 7 equals `last` — the shape both
/// `::` (last 0) and `::1` (last 1) share.
fn isAllZeroExceptLast(segs: [8]u16, last: u16) bool {
    for (segs[0..7]) |s| if (s != 0) return false;
    return segs[7] == last;
}

/// Parse a dotted-decimal IPv4 literal into 4 octets, or null if not one.
fn parseIpv4(s: []const u8) ?[4]u8 {
    var octets: [4]u8 = undefined;
    var count: u8 = 0;
    var start: usize = 0;
    for (s, 0..) |c, i| {
        if (c == '.') {
            if (count >= 3) return null;
            octets[count] = std.fmt.parseInt(u8, s[start..i], 10) catch return null;
            count += 1;
            start = i + 1;
        } else if (c < '0' or c > '9') return null;
    }
    if (count != 3) return null;
    octets[3] = std.fmt.parseInt(u8, s[start..], 10) catch return null;
    return octets;
}

/// Parse an IPv6 literal into 8 segments, supporting `::` elision and a trailing
/// embedded IPv4 (`::ffff:127.0.0.1`). Null if not a valid IPv6 literal.
fn parseIpv6(s: []const u8) ?[8]u16 {
    if (s.len == 0) return null;
    var segs: [8]u16 = .{ 0, 0, 0, 0, 0, 0, 0, 0 };

    if (std.mem.indexOf(u8, s, "::")) |dc| {
        const head = s[0..dc];
        const tail = s[dc + 2 ..];
        var head_buf: [8]u16 = undefined;
        const head_n = if (head.len == 0) 0 else (parseV6Run(head, &head_buf) orelse return null);
        var tail_buf: [8]u16 = undefined;
        const tail_n = if (tail.len == 0) 0 else (parseV6Run(tail, &tail_buf) orelse return null);
        if (head_n + tail_n > 8) return null; // `::` must stand for ≥1 zero group
        if (head_n + tail_n == 8) return null;
        for (0..head_n) |i| segs[i] = head_buf[i];
        for (0..tail_n) |i| segs[8 - tail_n + i] = tail_buf[i];
        return segs;
    }

    const n = parseV6Run(s, &segs) orelse return null;
    if (n != 8) return null;
    return segs;
}

/// Parse a colon-separated run of hex groups (with an optional trailing embedded
/// IPv4, which expands to two groups) into `out`. Returns the group count, or
/// null on malformed input.
fn parseV6Run(s: []const u8, out: *[8]u16) ?usize {
    var idx: usize = 0;
    var start: usize = 0;
    var i: usize = 0;
    while (i <= s.len) : (i += 1) {
        if (i == s.len or s[i] == ':') {
            const group = s[start..i];
            if (group.len == 0) return null; // empty group (e.g. trailing ':')
            // A dotted group is an embedded IPv4 → two segments; only valid last.
            if (std.mem.indexOfScalar(u8, group, '.') != null) {
                if (i != s.len) return null;
                const v4 = parseIpv4(group) orelse return null;
                if (idx + 2 > out.len) return null;
                out[idx] = (@as(u16, v4[0]) << 8) | v4[1];
                out[idx + 1] = (@as(u16, v4[2]) << 8) | v4[3];
                idx += 2;
                return idx;
            }
            if (idx >= out.len) return null;
            out[idx] = std.fmt.parseInt(u16, group, 16) catch return null;
            idx += 1;
            start = i + 1;
        }
    }
    return idx;
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing = std.testing;

fn expectBlocked(host: []const u8) !void {
    try testing.expect(isBlockedHostLiteral(host));
}

test "isBlockedHostLiteral mirrors net_security: 127/8 and 10/8 whole ranges" {
    try expectBlocked("127.0.0.1");
    try expectBlocked("127.255.255.255");
    try expectBlocked("10.0.0.0");
    try expectBlocked("10.255.255.255");
    try testing.expect(!isBlockedHostLiteral("8.8.8.8"));
    try testing.expect(!isBlockedHostLiteral("1.1.1.1"));
    try testing.expect(!isBlockedHostLiteral("example.com")); // hostname passes the IP checks
}

test "isBlockedHostLiteral covers the v4 SSRF ranges and their boundaries" {
    try expectBlocked("172.16.5.9");
    try expectBlocked("172.31.255.255");
    try expectBlocked("192.168.1.1");
    try expectBlocked("169.254.169.254"); // cloud metadata
    try expectBlocked("0.0.0.0");
    try expectBlocked("224.0.0.1"); // multicast
    try expectBlocked("255.255.255.255"); // broadcast
    // /12 + /16 boundaries that are public — must not over-block.
    try testing.expect(!isBlockedHostLiteral("172.15.0.1"));
    try testing.expect(!isBlockedHostLiteral("172.32.0.1"));
    try testing.expect(!isBlockedHostLiteral("169.253.0.1"));
    try testing.expect(!isBlockedHostLiteral("169.255.0.1"));
}

test "isBlockedHostLiteral covers the v6 SSRF ranges (bracketed + bare)" {
    try expectBlocked("[::1]"); // loopback
    try expectBlocked("::"); // unspecified
    try expectBlocked("[fc00::1]"); // unique-local
    try expectBlocked("[fd12::3]"); // unique-local (fd prefix)
    try expectBlocked("[fe80::1]"); // link-local
    try expectBlocked("[ff02::1]"); // multicast
    try expectBlocked("[::ffff:127.0.0.1]"); // IPv4-mapped loopback
    try expectBlocked("[::ffff:169.254.169.254]"); // IPv4-mapped metadata
    try testing.expect(!isBlockedHostLiteral("[2606:4700:4700::1111]")); // public v6
}

test "isBlockedHostLiteral strips an IPv6 zone id before the range check" {
    try expectBlocked("fe80::1%lo0");
    try expectBlocked("[fe80::1%25eth0]");
}

test "parseIpv4 rejects non-literals and out-of-range octets" {
    try testing.expect(parseIpv4("not-an-ip") == null);
    try testing.expect(parseIpv4("256.1.1.1") == null);
    try testing.expect(parseIpv4("1.2.3") == null);
    try testing.expect(parseIpv4("1.2.3.4.5") == null);
    try testing.expectEqual(@as(u8, 192), parseIpv4("192.168.1.1").?[0]);
}

test "parseIpv6 handles elision and embedded IPv4" {
    try testing.expectEqual(@as(u16, 1), parseIpv6("::1").?[7]);
    try testing.expectEqual(@as(u16, 0xfe80), parseIpv6("fe80::1").?[0]);
    try testing.expectEqual(@as(u16, 0xffff), parseIpv6("::ffff:127.0.0.1").?[5]);
    try testing.expect(parseIpv6("not::valid::twice") == null);
    try testing.expect(parseIpv6("gggg::1") == null);
}
