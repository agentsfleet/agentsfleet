//! Row → wire decoding shared by the two operator-plane runner reads
//! (`runners_list.zig`, `runner_get.zig`), so the list and the detail can
//! never disagree on how a runner row resolves: derived liveness, parsed
//! labels, and the M148 assigned-policy / capability / degraded-verdict
//! columns. `token_hash` and the stored auth state have no field anywhere
//! here, so emitting either is a compile error.

const std = @import("std");
const protocol = @import("contract").protocol;
const constants = @import("common");
const policy_row = @import("../runner/assigned_policy_row.zig");
const ec = @import("../../../errors/error_registry.zig");
const logging = @import("log");

const log = logging.scoped(.fleet_runner_row);

/// One fleet row as returned to the operator. `liveness` is derived,
/// `assigned_policy`/`achievable` decode from the row's columns (null = the
/// pre-policy row / no report yet), and the verdict rides verbatim.
pub const RunnerItem = struct {
    id: []const u8,
    host_id: []const u8,
    sandbox_tier: []const u8,
    admin_state: protocol.AdminState,
    liveness: protocol.RunnerLiveness,
    labels: []const []const u8,
    last_seen_at: i64,
    created_at: i64,
    assigned_policy: ?protocol.AssignedPolicy,
    achievable: ?protocol.CapabilityReport,
    degraded: bool,
    degraded_reason: ?[]const u8,
};

/// The six M148 columns every runner read appends in this order:
/// network_policy, registry_allowlist, worker_count, capability_report,
/// degraded, degraded_reason. The assigned tier rides earlier in each
/// statement, so it arrives as a parameter here.
pub const PolicyColumns = struct {
    assigned_policy: ?protocol.AssignedPolicy,
    achievable: ?protocol.CapabilityReport,
    degraded: bool,
    degraded_reason: ?[]const u8,
};

/// Derive runtime liveness from the stored `last_seen_at` + whether the runner
/// holds a live lease. Pure → unit-testable without a database. Order is
/// load-bearing: `busy` (live lease, actively renewing) is checked BEFORE the
/// offline threshold so a long-running execution is never falsely offline.
pub fn deriveLiveness(last_seen_at: i64, has_live_lease: bool, now_ms: i64) protocol.RunnerLiveness {
    if (last_seen_at == protocol.RUNNER_LAST_SEEN_NEVER) return .registered;
    if (has_live_lease) return .busy;
    if (now_ms - last_seen_at <= constants.RUNNER_OFFLINE_AFTER_MS) return .online;
    return .offline;
}

/// Decode the policy/verdict columns starting at `base`. Slices are decoded
/// into (or duped onto) `alloc` — the request arena — so nothing borrows the
/// query result past the caller's drain.
pub fn readPolicyColumns(alloc: std.mem.Allocator, row: anytype, tier_raw: []const u8, base: usize) !PolicyColumns {
    const network_raw = try row.get(?[]u8, base);
    const registry_raw = try row.get(?[]u8, base + 1);
    const worker_count = try row.get(i32, base + 2);
    const capability_raw = try row.get(?[]u8, base + 3);
    const degraded = try row.get(bool, base + 4);
    const reason_raw = try row.get(?[]u8, base + 5);
    const extra_binds_raw = try row.get(?[]u8, base + 6);
    return .{
        .assigned_policy = policy_row.decodePolicy(alloc, tier_raw, network_raw, registry_raw, worker_count, extra_binds_raw),
        .achievable = policy_row.decodeCapability(alloc, capability_raw),
        .degraded = degraded,
        .degraded_reason = if (reason_raw) |r| try alloc.dupe(u8, r) else null,
    };
}

/// The stored self-test verdict, re-exported so `runner_get` reaches it through
/// the same row-decoding façade it already uses for the policy tail rather than
/// importing across handler families.
pub const decodeSelftest = policy_row.decodeSelftest;

/// Build one list item, duping borrowed row slices into the request arena
/// (they outlive the query's deinit) and parsing the labels JSONB.
pub fn readItem(alloc: std.mem.Allocator, row: anytype, now_ms: i64) !RunnerItem {
    // Read the scalar columns first (fallible, no allocation), then dupe the
    // borrowed slices with an errdefer per owned slice — a decode error on a
    // later column frees the earlier dupes instead of leaking them on partial init.
    const raw_admin_state = try row.get([]u8, 3);
    const admin_state = std.meta.stringToEnum(protocol.AdminState, raw_admin_state) orelse return error.DbRowShape;
    const last_seen_at = try row.get(i64, 5);
    const created_at = try row.get(i64, 6);
    const has_live_lease = try row.get(bool, 7);
    const tier_raw = try row.get([]u8, 2);
    const policy = try readPolicyColumns(alloc, row, tier_raw, 8);
    errdefer if (policy.degraded_reason) |r| alloc.free(r);
    const id = try alloc.dupe(u8, try row.get([]u8, 0));
    errdefer alloc.free(id);
    const host_id = try alloc.dupe(u8, try row.get([]u8, 1));
    errdefer alloc.free(host_id);
    const sandbox_tier = try alloc.dupe(u8, tier_raw);
    errdefer alloc.free(sandbox_tier);
    return .{
        .id = id,
        .host_id = host_id,
        .sandbox_tier = sandbox_tier,
        .admin_state = admin_state,
        .labels = parseLabels(alloc, try row.get([]u8, 4)),
        .last_seen_at = last_seen_at,
        .created_at = created_at,
        .liveness = deriveLiveness(last_seen_at, has_live_lease, now_ms),
        .assigned_policy = policy.assigned_policy,
        .achievable = policy.achievable,
        .degraded = policy.degraded,
        .degraded_reason = policy.degraded_reason,
    };
}

/// Drain the row iterator into owned items. A row that fails to decode is
/// skipped (logged) — one bad row must not abort the page — but a mid-iteration
/// transport error propagates so the caller fails closed instead of returning a
/// partial page. `rows` is anything exposing `next() !?Row`; tests drive every
/// branch with a fake iterator. `alloc` is the caller-owned request arena, so
/// partial items on the error path are reclaimed when that arena is released.
pub fn collectItems(alloc: std.mem.Allocator, rows: anytype, now_ms: i64) ![]RunnerItem {
    var items: std.ArrayList(RunnerItem) = .empty;
    errdefer items.deinit(alloc);
    while (try rows.next()) |row| {
        const item = readItem(alloc, row, now_ms) catch |err| {
            log.warn("row_decode_skipped", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err) });
            continue;
        };
        try items.append(alloc, item);
    }
    return items.toOwnedSlice(alloc);
}

/// Parse the stored labels JSONB (a JSON array of strings) into owned slices.
/// A malformed value degrades to an empty set rather than failing the read.
pub fn parseLabels(alloc: std.mem.Allocator, text: []const u8) []const []const u8 {
    return std.json.parseFromSliceLeaky([]const []const u8, alloc, text, .{ .allocate = .alloc_always }) catch &.{};
}

// ── Tests (fake row/iterator drive every branch without a database) ─────────

const MS_PER_SECOND = 1000;
const TEST_REASON = "landlock unavailable";
const EMPTY_JSON_ARRAY = "[]";
const TEST_CAP_JSON =
    \\{"landlock":false,"seccomp":true,"cgroup_controllers":["cpu","memory","pids"],"bubblewrap":true,"egress_enforcement":false}
;

const FakeRow = struct {
    const Self = @This();

    id: []const u8 = "r1",
    host_id: []const u8 = "h1",
    sandbox_tier: []const u8 = "landlock_full",
    admin_state: []const u8 = "active",
    labels_json: []const u8 = EMPTY_JSON_ARRAY,
    last_seen_at: i64 = 0,
    created_at: i64 = 0,
    has_live_lease: bool = false,
    network_policy: ?[]const u8 = "allow_all",
    registry_json: ?[]const u8 = EMPTY_JSON_ARRAY,
    worker_count: i32 = 1,
    capability_json: ?[]const u8 = null,
    degraded: bool = false,
    degraded_reason: ?[]const u8 = null,
    extra_binds_json: ?[]const u8 = null,
    fail_at: ?usize = null, // inject a decode error at this column index

    fn get(self: *const Self, comptime T: type, col: usize) !T {
        if (self.fail_at) |fc| {
            if (fc == col) return error.TestDecode;
        }
        if (T == []u8) return @constCast(switch (col) {
            0 => self.id,
            1 => self.host_id,
            2 => self.sandbox_tier,
            3 => self.admin_state,
            4 => self.labels_json,
            else => unreachable,
        });
        if (T == ?[]u8) return @constCast(switch (col) {
            8 => self.network_policy orelse return null,
            9 => self.registry_json orelse return null,
            11 => self.capability_json orelse return null,
            13 => self.degraded_reason orelse return null,
            14 => self.extra_binds_json orelse return null,
            else => unreachable,
        });
        if (T == i64) return switch (col) {
            5 => self.last_seen_at,
            6 => self.created_at,
            else => unreachable,
        };
        if (T == i32) return switch (col) {
            10 => self.worker_count,
            else => unreachable,
        };
        if (T == bool) return switch (col) {
            7 => self.has_live_lease,
            12 => self.degraded,
            else => unreachable,
        };
        unreachable;
    }
};

const FakeRows = struct {
    const Self = @This();

    rows: []const FakeRow,
    idx: usize = 0,
    fail_after: ?usize = null, // transport error once this many rows are yielded

    fn next(self: *Self) !?FakeRow {
        if (self.fail_after) |n| {
            if (self.idx == n) return error.TestTransport;
        }
        if (self.idx >= self.rows.len) return null;
        const r = self.rows[self.idx];
        self.idx += 1;
        return r;
    }
};

test "collectItems: a clean read returns every row in order, policy decoded" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var rows = FakeRows{ .rows = &.{ .{ .id = "a" }, .{ .id = "b" } } };
    const items = try collectItems(arena.allocator(), &rows, MS_PER_SECOND);
    try std.testing.expectEqual(@as(usize, 2), items.len);
    try std.testing.expectEqualStrings("a", items[0].id);
    try std.testing.expectEqual(protocol.AdminState.active, items[0].admin_state);
    const assigned = items[0].assigned_policy orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(protocol.SandboxTier.landlock_full, assigned.sandbox_tier);
    try std.testing.expect(!items[0].degraded);
    try std.testing.expectEqualStrings("b", items[1].id);
}

test "readItem: a degraded row carries verdict, reason, and the stored report" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fake = FakeRow{ .degraded = true, .degraded_reason = TEST_REASON, .capability_json = TEST_CAP_JSON };
    const item = try readItem(arena.allocator(), fake, MS_PER_SECOND);
    try std.testing.expect(item.degraded);
    try std.testing.expectEqualStrings(TEST_REASON, item.degraded_reason orelse return error.TestUnexpectedResult);
    const cap = item.achievable orelse return error.TestUnexpectedResult;
    try std.testing.expect(!cap.landlock);
    try std.testing.expectEqual(@as(usize, 3), cap.cgroup_controllers.len);
}

test "readItem: the operator's extra binds ride the row out to the dashboard at their modes" {
    // Dimension 4.1 — the operator surface must read back what it assigned.
    // Without this column on the read the page would render an assignment it
    // had just stored as empty, and an operator would re-add a bind that was
    // already there.
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fake = FakeRow{ .extra_binds_json =
        \\[{"path":"/srv/fonts"},{"path":"/srv/models","mode":"read_write","note":"shared model cache"}]
    };
    const item = try readItem(arena.allocator(), fake, MS_PER_SECOND);
    const p = item.assigned_policy orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(usize, 2), p.extra_binds.len);
    try std.testing.expectEqualStrings("/srv/fonts", p.extra_binds[0].path);
    try std.testing.expectEqual(protocol.BindMode.read_only, p.extra_binds[0].mode);
    try std.testing.expectEqual(protocol.BindMode.read_write, p.extra_binds[1].mode);
    try std.testing.expectEqualStrings("shared model cache", p.extra_binds[1].note);
}

test "readItem: a row with no assigned binds reads the baseline, not a null policy" {
    // A NULL `extra_binds` is every runner enrolled before `schema/670`. It
    // must not fail the policy decode — those runners still lease.
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const item = try readItem(arena.allocator(), FakeRow{}, MS_PER_SECOND);
    const p = item.assigned_policy orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(usize, 0), p.extra_binds.len);
}

test "readItem: a pre-policy row (NULL network) reads assigned_policy = null, never defaults" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fake = FakeRow{ .network_policy = null };
    const item = try readItem(arena.allocator(), fake, MS_PER_SECOND);
    try std.testing.expect(item.assigned_policy == null);
    try std.testing.expect(item.achievable == null);
}

test "collectItems: a row that fails to decode is skipped; the rest survive" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var rows = FakeRows{ .rows = &.{ .{ .id = "a" }, .{ .id = "bad", .fail_at = 0 }, .{ .id = "c" } } };
    const items = try collectItems(arena.allocator(), &rows, MS_PER_SECOND);
    try std.testing.expectEqual(@as(usize, 2), items.len);
    try std.testing.expectEqualStrings("a", items[0].id);
    try std.testing.expectEqualStrings("c", items[1].id);
}

test "collectItems: a mid-iteration transport error propagates (caller fails closed)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var rows = FakeRows{ .rows = &.{ .{ .id = "a" }, .{ .id = "b" } }, .fail_after = 1 };
    try std.testing.expectError(error.TestTransport, collectItems(arena.allocator(), &rows, MS_PER_SECOND));
}

test "readItem: a mid-decode column error frees the slices duped before it" {
    // Raw testing allocator (no arena): the leak detector fires if the errdefer
    // chain misses a dupe. fail_at=1 errors on host_id after BOTH the id dupe
    // and the degraded-reason dupe — each must be freed by its errdefer.
    // registry/capability stay null so no JSON parse allocates outside them.
    const fake = FakeRow{ .registry_json = null, .degraded_reason = TEST_REASON, .fail_at = 1 };
    try std.testing.expectError(error.TestDecode, readItem(std.testing.allocator, fake, MS_PER_SECOND));
}

test "parseLabels: a JSON array of strings parses to owned slices; malformed degrades to empty" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const labels = parseLabels(arena.allocator(), "[\"gpu\",\"prod\"]");
    try std.testing.expectEqual(@as(usize, 2), labels.len);
    try std.testing.expectEqualStrings("gpu", labels[0]);
    try std.testing.expectEqual(@as(usize, 0), parseLabels(arena.allocator(), "{not valid").len);
}
