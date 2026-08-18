//! Row → wire decoding for the assigned-policy columns on `fleet.runners`.
//!
//! One decoder shared by the self read, the heartbeat reply, and the operator
//! surfaces, so every caller resolves a row to the same `?AssignedPolicy`: a
//! missing or unparseable column yields null — the runner side then fails
//! closed and refuses to lease, and the reconciliation names the gap. A
//! partial assignment is never silently completed with defaults.

const std = @import("std");
const protocol = @import("contract").protocol;

/// Decode the assigned-policy columns into the wire payload. `alloc` should be
/// the request-scoped arena — parsed registry and bind slices borrow from it
/// and die with the request. A NULL network policy or registry (a row from
/// before the policy columns existed) or any unparseable value → null.
///
/// `extra_binds` is the exception to that fail-closed rule: NULL resolves to an
/// empty list, because an absent extra list is the NORMAL state. Every runner
/// enrolled before `schema/670` reads NULL, and refusing to decode those would
/// stop the whole fleet leasing over a list nobody assigned.
pub fn decodePolicy(
    alloc: std.mem.Allocator,
    tier_raw: []const u8,
    network_raw: ?[]const u8,
    registry_json: ?[]const u8,
    worker_count_raw: i32,
    extra_binds_json: ?[]const u8,
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
        .extra_binds = decodeExtraBinds(alloc, extra_binds_json),
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

/// Rebuild a stored verdict from its four columns, or null when the runner has
/// never self-tested. The columns are written together by one statement
/// (`UPDATE_RUNNER_SELFTEST`), so a partial set means a row edited out-of-band:
/// that reads as no verdict rather than as a half-verdict, because rendering
/// checks without the policy they ran under is exactly the stale-result
/// confusion Dimension 1.3 exists to prevent.
pub fn decodeSelftest(
    alloc: std.mem.Allocator,
    checks_json: ?[]const u8,
    all_ok: ?bool,
    tier_raw: ?[]const u8,
    network_raw: ?[]const u8,
) ?protocol.SelftestReport {
    const raw = checks_json orelse return null;
    const ok = all_ok orelse return null;
    const tier = tier_raw orelse return null;
    const network = network_raw orelse return null;
    // alloc_always: same borrowed-buffer rule as decodeCapability — the check
    // names and details must own their bytes past the row's deinit.
    const checks = std.json.parseFromSliceLeaky([]const protocol.SelftestCheck, alloc, raw, .{ .ignore_unknown_fields = true, .allocate = .alloc_always }) catch return null;
    return .{
        .checks = checks,
        .all_ok = ok,
        .sandbox_tier = alloc.dupe(u8, tier) catch return null,
        .network_policy = alloc.dupe(u8, network) catch return null,
    };
}

/// The operator's extra binds, or an empty list when none are assigned. A
/// stored value that no longer parses reads the same as none: the sandbox falls
/// back to the daemon-owned baseline, which is the safe direction — a garbled
/// row can never widen a boundary, only fail to widen one. The runner validates
/// whatever does decode before it reaches an argv.
fn decodeExtraBinds(alloc: std.mem.Allocator, extra_binds_json: ?[]const u8) []const protocol.ExtraBind {
    const raw = extra_binds_json orelse return &.{};
    // alloc_always: same borrowed-buffer rule as decodeCapability — the paths
    // and notes must own their bytes past the row's deinit.
    return std.json.parseFromSliceLeaky([]const protocol.ExtraBind, alloc, raw, .{ .ignore_unknown_fields = true, .allocate = .alloc_always }) catch &.{};
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
    , 4, null) orelse return error.TestUnexpectedResult;
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
    try std.testing.expect(decodePolicy(a, "landlock_full", null, "[]", 1, null) == null); // pre-policy row
    try std.testing.expect(decodePolicy(a, "landlock_full", "open_sesame", "[]", 1, null) == null); // unknown network
    try std.testing.expect(decodePolicy(a, "quantum_cage", "allow_all", "[]", 1, null) == null); // unknown tier
    try std.testing.expect(decodePolicy(a, "landlock_full", "allow_all", "not-json", 1, null) == null); // bad registry
    try std.testing.expect(decodePolicy(a, "landlock_full", "allow_all", null, 1, null) == null); // registry never assigned
}

test "decodePolicy carries the operator's extra binds off the row at their assigned modes" {
    // Dimension 4.1 — the stored list is what the heartbeat delivers. Without
    // this decode the column was written and never read back, so every beat
    // handed the host an empty list and the assignment did nothing.
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const p = decodePolicy(arena.allocator(), "landlock_full", "allow_all", "[]", 2,
        \\[{"path":"/srv/fonts"},{"path":"/srv/models","mode":"read_write","note":"shared model cache"}]
    ) orelse return error.TestUnexpectedResult;

    try std.testing.expectEqual(@as(usize, 2), p.extra_binds.len);
    try std.testing.expectEqualStrings("/srv/fonts", p.extra_binds[0].path);
    // An entry that never named a mode decodes read-only — access does not
    // widen by omission, on a stored row exactly as on the wire.
    try std.testing.expectEqual(protocol.BindMode.read_only, p.extra_binds[0].mode);
    try std.testing.expectEqual(protocol.BindMode.read_write, p.extra_binds[1].mode);
    try std.testing.expectEqualStrings("shared model cache", p.extra_binds[1].note);
}

test "decodePolicy resolves an absent or garbled bind list to the baseline, not to null" {
    // A NULL column is every runner enrolled before `schema/670`; garbage is a
    // row edited out-of-band. Both mean "no operator additions" — the policy
    // still decodes, because refusing it would stop the fleet leasing, and
    // neither case can widen the sandbox.
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const absent = decodePolicy(a, "landlock_full", "allow_all", "[]", 1, null) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(usize, 0), absent.extra_binds.len);

    const garbled = decodePolicy(a, "landlock_full", "allow_all", "[]", 1, "{{not-json") orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(usize, 0), garbled.extra_binds.len);
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

test "decodeSelftest parses a stored verdict and keeps its policy alongside it" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const report = decodeSelftest(
        a,
        \\[{"name":"a hostname resolves inside the sandbox","ok":false,"detail":"the resolver did not answer","future":1}]
    ,
        false,
        "landlock_full",
        "deny_all_egress",
    ) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(usize, 1), report.checks.len);
    try std.testing.expect(!report.all_ok);
    try std.testing.expect(!report.checks[0].ok);
    // The policy travels WITH the verdict — the page labels a mismatch stale
    // rather than presenting an old result against the current assignment.
    try std.testing.expectEqualStrings("landlock_full", report.sandbox_tier);
    try std.testing.expectEqualStrings("deny_all_egress", report.network_policy);
}

test "decodeSelftest reads a row missing any one column as no verdict at all" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const checks = "[]";
    // A partial verdict is never completed with defaults: every column must be
    // present or the row reads as never-self-tested (the same fail-closed rule
    // decodePolicy applies), so each NULL is proven independently.
    try std.testing.expect(decodeSelftest(a, null, true, "landlock_full", "allow_all") == null);
    try std.testing.expect(decodeSelftest(a, checks, null, "landlock_full", "allow_all") == null);
    try std.testing.expect(decodeSelftest(a, checks, true, null, "allow_all") == null);
    try std.testing.expect(decodeSelftest(a, checks, true, "landlock_full", null) == null);
}

test "decodeSelftest reads an unparseable verdict as none rather than surfacing a partial one" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    try std.testing.expect(decodeSelftest(a, "{{nope", true, "landlock_full", "allow_all") == null);
    // Right JSON shape, wrong type for `ok` — still refused whole.
    try std.testing.expect(decodeSelftest(a, "[{\"name\":\"n\",\"ok\":\"yes\",\"detail\":\"d\"}]", true, "landlock_full", "allow_all") == null);
}

test "decodeSelftest yields no verdict when the arena cannot own the policy strings" {
    // The two dupes own their bytes past the row's deinit; if either cannot be
    // made, returning a report pointing at borrowed row memory would be a
    // use-after-free, so the decoder drops the whole verdict instead.
    const checks = "[]";
    var index: usize = 0;
    while (index < 8) : (index += 1) {
        var fa = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = index });
        var arena = std.heap.ArenaAllocator.init(fa.allocator());
        defer arena.deinit();
        // Either it decoded fully or it refused — never a report with a
        // half-owned string.
        if (decodeSelftest(arena.allocator(), checks, true, "landlock_full", "allow_all")) |r| {
            try std.testing.expectEqualStrings("landlock_full", r.sandbox_tier);
            try std.testing.expectEqualStrings("allow_all", r.network_policy);
        }
    }
}
