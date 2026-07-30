//! Row → wire decoding for the assigned-policy columns on `fleet.runners`.
//!
//! One decoder shared by the self read, the heartbeat reply, and the operator
//! surfaces, so every caller resolves a row to the same `?AssignedPolicy`: a
//! missing or unparseable column yields null — the runner side then fails
//! closed and refuses to lease, and the reconciliation names the gap. A
//! partial assignment is never silently completed with defaults.

const std = @import("std");
const protocol = @import("contract").protocol;

/// Decode the four assigned-policy columns into the wire payload. `alloc`
/// should be the request-scoped arena — parsed registry slices borrow from it
/// and die with the request. A NULL network policy or registry (a row from
/// before the policy columns existed) or any unparseable value → null.
pub fn decodePolicy(
    alloc: std.mem.Allocator,
    tier_raw: []const u8,
    network_raw: ?[]const u8,
    registry_json: ?[]const u8,
    worker_count_raw: i32,
) ?protocol.AssignedPolicy {
    const tier = std.meta.stringToEnum(protocol.SandboxTier, tier_raw) orelse return null;
    const network_str = network_raw orelse return null;
    const network = std.meta.stringToEnum(protocol.NetworkPolicy, network_str) orelse return null;
    const registry = decodeRegistry(alloc, registry_json) orelse return null;
    return .{
        .sandbox_tier = tier,
        .network_policy = network,
        .registry_allowlist = registry,
        .worker_count = clampWorkerCount(worker_count_raw),
    };
}

/// The stored capability report (JSONB text, written only by the heartbeat
/// path), or null when the runner has not reported yet — or the stored value
/// no longer parses, which reads the same: no proven capability.
pub fn decodeCapability(alloc: std.mem.Allocator, report_json: ?[]const u8) ?protocol.CapabilityReport {
    const raw = report_json orelse return null;
    // alloc_always: without it, unescaped parsed strings are SUBSLICES of
    // `raw` — the borrowed pg row buffer — and dangle once the query drains.
    return std.json.parseFromSliceLeaky(protocol.CapabilityReport, alloc, raw, .{ .ignore_unknown_fields = true, .allocate = .alloc_always }) catch null;
}

fn decodeRegistry(alloc: std.mem.Allocator, registry_json: ?[]const u8) ?[]const []const u8 {
    const raw = registry_json orelse return null;
    // alloc_always: same borrowed-buffer rule as decodeCapability — the
    // registry hosts must own their bytes past the row's deinit.
    return std.json.parseFromSliceLeaky([]const []const u8, alloc, raw, .{ .allocate = .alloc_always }) catch null;
}

/// Clamp a count into the shared bounds — a row edited out-of-band can never
/// size a pool outside them, mirroring the write-side clamp at assignment.
pub fn clampWorkerCount(raw: i32) u32 {
    if (raw < 1) return protocol.MIN_WORKER_COUNT;
    return std.math.clamp(@as(u32, @intCast(raw)), protocol.MIN_WORKER_COUNT, protocol.MAX_WORKER_COUNT);
}

test "decodePolicy resolves a fully assigned row" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const p = decodePolicy(arena.allocator(), "landlock_full", "allow_all",
        \\["registry.npmjs.org","pypi.org"]
    , 4) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(protocol.SandboxTier.landlock_full, p.sandbox_tier);
    try std.testing.expectEqual(protocol.NetworkPolicy.allow_all, p.network_policy);
    try std.testing.expectEqual(@as(usize, 2), p.registry_allowlist.len);
    try std.testing.expectEqualStrings("pypi.org", p.registry_allowlist[1]);
    try std.testing.expectEqual(@as(u32, 4), p.worker_count);
}

test "decodePolicy fails closed to null on any missing or unparseable column" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    try std.testing.expect(decodePolicy(a, "landlock_full", null, "[]", 1) == null); // pre-policy row
    try std.testing.expect(decodePolicy(a, "landlock_full", "open_sesame", "[]", 1) == null); // unknown network
    try std.testing.expect(decodePolicy(a, "quantum_cage", "allow_all", "[]", 1) == null); // unknown tier
    try std.testing.expect(decodePolicy(a, "landlock_full", "allow_all", "not-json", 1) == null); // bad registry
    try std.testing.expect(decodePolicy(a, "landlock_full", "allow_all", null, 1) == null); // registry never assigned
}

test "clampWorkerCount clamps into the shared bounds" {
    try std.testing.expectEqual(protocol.MIN_WORKER_COUNT, clampWorkerCount(0));
    try std.testing.expectEqual(protocol.MIN_WORKER_COUNT, clampWorkerCount(-7));
    try std.testing.expectEqual(protocol.MAX_WORKER_COUNT, clampWorkerCount(@as(i32, @intCast(protocol.MAX_WORKER_COUNT + 1))));
    try std.testing.expectEqual(@as(u32, 8), clampWorkerCount(8));
}

test "decodeCapability parses a stored report, tolerates unknown fields, nulls on garbage" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const cap = decodeCapability(a,
        \\{"landlock":true,"seccomp":false,"cgroup_controllers":["cpu","memory"],"bubblewrap":true,"egress_enforcement":false,"future_field":1}
    ) orelse return error.TestUnexpectedResult;
    try std.testing.expect(cap.landlock);
    try std.testing.expect(!cap.seccomp);
    try std.testing.expectEqual(@as(usize, 2), cap.cgroup_controllers.len);
    try std.testing.expect(decodeCapability(a, null) == null);
    try std.testing.expect(decodeCapability(a, "{{nope") == null);
}
