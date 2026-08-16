//! The daemon's single owner of the control-plane-assigned policy.
//!
//! The control loop writes it from each heartbeat reply (deep-copied out of
//! the reply's parse before that parse is freed); every worker reads a
//! snapshot at its lease boundary. A missing, malformed, or gate-refused
//! assignment stores NOTHING — a null holder is the fail-closed "lease
//! nothing" verdict, never a fallback to defaults or to the previous policy.

const AppliedPolicy = @This();

/// Protects `current`: written by the control loop on a policy change, read by
/// every worker at its lease boundary. Values are copied in and out under the
/// lock; no caller ever holds a reference into the guarded value.
mutex: common.Mutex = .{},
alloc: std.mem.Allocator,
current: ?protocol.AssignedPolicy = null,
/// Control-plane verdict from the last heartbeat: assigned exceeds what this
/// host can enforce. Workers refuse to lease while set — the runner-side half
/// of "an unmet assignment leases nothing" (the control plane also issues
/// nothing). Atomic: read per poll, written only by the control loop.
degraded: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),

/// The heartbeat reply as the daemon reads it (the client parses into this).
/// `assigned_policy` stays RAW JSON: a malformed assignment must fail closed
/// (refuse to lease) without failing the heartbeat that carried it, or an old
/// runner meeting a newer policy vocabulary would stall its own liveness.
pub const HeartbeatReplyRaw = struct {
    status: protocol.HeartbeatStatus,
    assigned_policy: ?std.json.Value = null,
    degraded: bool = false,
    degraded_reason: ?[]const u8 = null,
};

pub fn init(alloc: std.mem.Allocator) AppliedPolicy {
    return .{ .alloc = alloc };
}

pub fn deinit(self: *AppliedPolicy) void {
    self.mutex.lock();
    if (self.current) |p| freePolicy(self.alloc, p);
    self.current = null;
    self.mutex.unlock();
    self.* = undefined;
}

/// Outcome of feeding one heartbeat's `assigned_policy` into the holder.
///   unchanged — same value (or still absent) as last beat; nothing to log.
///   applied   — a (new) policy is now held; run the apply-time gates.
///   cleared   — the row carries no assignment any more; leasing stops.
///   invalid   — this beat's policy did not decode; holder is now null
///               (fail closed — an unreadable policy must stop leasing,
///               never keep running under the previous one).
pub const ApplyOutcome = enum { unchanged, applied, cleared, invalid };

/// Decode + store the heartbeat's raw `assigned_policy` value.
pub fn apply(self: *AppliedPolicy, raw: ?std.json.Value) ApplyOutcome {
    const value = raw orelse return self.store(null, .cleared);
    var arena = std.heap.ArenaAllocator.init(self.alloc);
    defer arena.deinit();
    var decoded = std.json.parseFromValueLeaky(protocol.AssignedPolicy, arena.allocator(), value, .{ .ignore_unknown_fields = true }) catch
        return self.store(null, .invalid);
    // The host-side half of the shared clamp: even a compromised or buggy
    // control plane can never size the pool outside the bounds.
    decoded.worker_count = std.math.clamp(decoded.worker_count, protocol.MIN_WORKER_COUNT, protocol.MAX_WORKER_COUNT);
    return self.store(decoded, .applied);
}

/// Drop the held policy (an apply-time gate refused it). Leasing stops until
/// a new assignment arrives.
pub fn clear(self: *AppliedPolicy) void {
    _ = self.store(null, .cleared);
}

pub fn setDegraded(self: *AppliedPolicy, v: bool) void {
    self.degraded.store(v, .seq_cst);
}

pub fn isDegraded(self: *AppliedPolicy) bool {
    return self.degraded.load(.seq_cst);
}

/// Deep copy of the held policy for one lease, or null when nothing is held
/// (or the copy failed — treated identically: lease nothing). Caller frees
/// with `freePolicy(alloc, p)`.
pub fn snapshot(self: *AppliedPolicy, alloc: std.mem.Allocator) ?protocol.AssignedPolicy {
    self.mutex.lock();
    defer self.mutex.unlock();
    const held = self.current orelse return null;
    return dupePolicy(alloc, held) catch null;
}

/// The held worker count, or null when no policy is applied. The control loop
/// sizes the pool from this; workers soft-shrink against it per lease.
pub fn currentWorkerCount(self: *AppliedPolicy) ?u32 {
    self.mutex.lock();
    defer self.mutex.unlock();
    const held = self.current orelse return null;
    return held.worker_count;
}

fn store(self: *AppliedPolicy, incoming: ?protocol.AssignedPolicy, outcome: ApplyOutcome) ApplyOutcome {
    self.mutex.lock();
    defer self.mutex.unlock();
    if (eqlOpt(self.current, incoming)) return .unchanged;
    const replacement: ?protocol.AssignedPolicy = if (incoming) |p|
        // Copy failure is fail-closed: hold nothing rather than a stale policy.
        dupePolicy(self.alloc, p) catch null
    else
        null;
    if (self.current) |old| freePolicy(self.alloc, old);
    self.current = replacement;
    if (incoming != null and replacement == null) return .invalid;
    return outcome;
}

fn eqlOpt(a: ?protocol.AssignedPolicy, b: ?protocol.AssignedPolicy) bool {
    if (a == null and b == null) return true;
    if (a == null or b == null) return false;
    return eqlPolicy(a.?, b.?);
}

fn eqlPolicy(a: protocol.AssignedPolicy, b: protocol.AssignedPolicy) bool {
    if (a.sandbox_tier != b.sandbox_tier) return false;
    if (a.network_policy != b.network_policy) return false;
    if (a.worker_count != b.worker_count) return false;
    if (a.registry_allowlist.len != b.registry_allowlist.len) return false;
    for (a.registry_allowlist, b.registry_allowlist) |x, y| {
        if (!std.mem.eql(u8, x, y)) return false;
    }
    return true;
}

fn dupePolicy(alloc: std.mem.Allocator, p: protocol.AssignedPolicy) !protocol.AssignedPolicy {
    const list = try alloc.alloc([]const u8, p.registry_allowlist.len);
    var duped: usize = 0;
    errdefer {
        for (list[0..duped]) |s| alloc.free(s);
        alloc.free(list);
    }
    for (p.registry_allowlist) |host| {
        list[duped] = try alloc.dupe(u8, host);
        duped += 1;
    }
    return .{
        .sandbox_tier = p.sandbox_tier,
        .network_policy = p.network_policy,
        .registry_allowlist = list,
        .worker_count = p.worker_count,
    };
}

/// Free a `snapshot` (or internal) deep copy. Caller passes the same
/// allocator the snapshot was taken with.
pub fn freePolicy(alloc: std.mem.Allocator, p: protocol.AssignedPolicy) void {
    for (p.registry_allowlist) |s| alloc.free(s);
    alloc.free(p.registry_allowlist);
}

const std = @import("std");
const common = @import("common");
const protocol = @import("contract").protocol;

fn testValue(alloc: std.mem.Allocator, json: []const u8) !std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, alloc, json, .{});
}

test "apply stores a decoded policy; identical re-apply is unchanged" {
    const a = std.testing.allocator;
    var holder = AppliedPolicy.init(a);
    defer holder.deinit();

    const v = try testValue(a,
        \\{"sandbox_tier":"landlock_full","network_policy":"allow_all","registry_allowlist":["pypi.org"],"worker_count":3}
    );
    defer v.deinit();
    try std.testing.expectEqual(ApplyOutcome.applied, holder.apply(v.value));
    try std.testing.expectEqual(@as(?u32, 3), holder.currentWorkerCount());
    try std.testing.expectEqual(ApplyOutcome.unchanged, holder.apply(v.value));

    const snap = holder.snapshot(a) orelse return error.TestUnexpectedResult;
    defer freePolicy(a, snap);
    try std.testing.expectEqual(protocol.SandboxTier.landlock_full, snap.sandbox_tier);
    try std.testing.expectEqualStrings("pypi.org", snap.registry_allowlist[0]);
}

test "a changed assignment applies; a null one clears; null again is unchanged" {
    const a = std.testing.allocator;
    var holder = AppliedPolicy.init(a);
    defer holder.deinit();

    const v1 = try testValue(a,
        \\{"sandbox_tier":"dev_none","network_policy":"deny_all_egress","registry_allowlist":[],"worker_count":1}
    );
    defer v1.deinit();
    try std.testing.expectEqual(ApplyOutcome.applied, holder.apply(v1.value));

    const v2 = try testValue(a,
        \\{"sandbox_tier":"dev_none","network_policy":"deny_all_egress","registry_allowlist":[],"worker_count":2}
    );
    defer v2.deinit();
    try std.testing.expectEqual(ApplyOutcome.applied, holder.apply(v2.value));
    try std.testing.expectEqual(@as(?u32, 2), holder.currentWorkerCount());

    try std.testing.expectEqual(ApplyOutcome.cleared, holder.apply(null));
    try std.testing.expect(holder.snapshot(a) == null);
    try std.testing.expectEqual(ApplyOutcome.unchanged, holder.apply(null));
}

test "test_malformed_assignment_fails_closed: holder empties and leasing has nothing to read" {
    const a = std.testing.allocator;
    var holder = AppliedPolicy.init(a);
    defer holder.deinit();

    const good = try testValue(a,
        \\{"sandbox_tier":"landlock_full","network_policy":"allow_all","registry_allowlist":[],"worker_count":1}
    );
    defer good.deinit();
    try std.testing.expectEqual(ApplyOutcome.applied, holder.apply(good.value));

    // An unknown tier must not keep the previous policy alive — refuse instead.
    const bad = try testValue(a,
        \\{"sandbox_tier":"quantum_cage","network_policy":"allow_all","registry_allowlist":[],"worker_count":1}
    );
    defer bad.deinit();
    try std.testing.expectEqual(ApplyOutcome.invalid, holder.apply(bad.value));
    try std.testing.expect(holder.snapshot(a) == null);
    try std.testing.expect(holder.currentWorkerCount() == null);
}

test "snapshot is an independent deep copy (mutating the holder later is safe)" {
    const a = std.testing.allocator;
    var holder = AppliedPolicy.init(a);
    defer holder.deinit();

    const v = try testValue(a,
        \\{"sandbox_tier":"container_nested","network_policy":"allow_all","registry_allowlist":["crates.io"],"worker_count":4}
    );
    defer v.deinit();
    try std.testing.expectEqual(ApplyOutcome.applied, holder.apply(v.value));

    const snap = holder.snapshot(a) orelse return error.TestUnexpectedResult;
    defer freePolicy(a, snap);
    holder.clear();
    // The snapshot's strings survive the holder's clear — owned, not borrowed.
    try std.testing.expectEqualStrings("crates.io", snap.registry_allowlist[0]);
}

test "a snapshot that cannot be fully duplicated frees its partial copy and reads null" {
    // The control loop calls `snapshot` per lease. A failure partway through the
    // allowlist copy must free the strings already duped rather than strand them
    // — a leak here recurs on every lease, for the life of the daemon. Two hosts
    // so the failure lands with one already copied; testing.allocator underneath
    // fails the test if it survives.
    const a = std.testing.allocator;
    var holder = AppliedPolicy.init(a);
    defer holder.deinit();

    const v = try testValue(a,
        \\{"sandbox_tier":"container_nested","network_policy":"allow_list_egress","registry_allowlist":["crates.io","registry.npmjs.org"],"worker_count":2}
    );
    defer v.deinit();
    try std.testing.expectEqual(ApplyOutcome.applied, holder.apply(v.value));

    for (0..3) |fail_index| {
        var fa = std.testing.FailingAllocator.init(a, .{ .fail_index = fail_index });
        // Null, never a partial policy: the caller reads "no snapshot" and
        // retries, instead of leasing against half an allowlist.
        try std.testing.expect(holder.snapshot(fa.allocator()) == null);
    }
    // The holder itself is untouched by a failed snapshot.
    const good = holder.snapshot(a) orelse return error.TestUnexpectedResult;
    defer freePolicy(a, good);
    try std.testing.expectEqual(@as(usize, 2), good.registry_allowlist.len);
}
