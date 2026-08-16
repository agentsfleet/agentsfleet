//! The bind-list arms of `AppliedPolicy`: change detection across an assigned
//! bind list, and the deep copy a snapshot takes of it.
//!
//! Split from `AppliedPolicy.zig` rather than added inline — that file sits nine
//! lines under the 350-line bound (RULE FLL) and these proofs are longer than
//! the room left.
//!
//! Why these arms are worth their own file: the holder decides whether a lease
//! rebuilds its mounts. A bind list that compares equal when it is not leaves
//! every lease running the OLD mount set until some unrelated field moves, and a
//! snapshot that copies the list shallowly hands a worker pointers the holder
//! frees underneath it.

const std = @import("std");
const protocol = @import("contract").protocol;
const AppliedPolicy = @import("AppliedPolicy.zig");
const ApplyOutcome = AppliedPolicy.ApplyOutcome;
const freePolicy = AppliedPolicy.freePolicy;

fn testValue(alloc: std.mem.Allocator, json: []const u8) !std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, alloc, json, .{});
}

/// One assignment carrying a single bind, spelled by its three operator-visible
/// fields so each test varies exactly one of them.
fn policyJson(comptime path: []const u8, comptime mode: []const u8, comptime note: []const u8) []const u8 {
    return "{\"sandbox_tier\":\"landlock_full\",\"network_policy\":\"allow_all\"," ++
        "\"registry_allowlist\":[],\"worker_count\":1,\"extra_binds\":" ++
        "[{\"path\":\"" ++ path ++ "\",\"mode\":\"" ++ mode ++ "\",\"note\":\"" ++ note ++ "\"}]}";
}

test "test_bind_list_change_is_detected_per_field" {
    // Each re-apply differs from the last in ONE field. If any comparison arm is
    // missing, that re-apply reads `.unchanged` and the running leases keep the
    // superseded mount set — the failure is silent, which is why every field is
    // driven separately rather than through one combined assignment.
    const a = std.testing.allocator;
    var holder = AppliedPolicy.init(a);
    defer holder.deinit();

    const base = try testValue(a, policyJson("/srv/models", "read_only", "shared model cache"));
    defer base.deinit();
    try std.testing.expectEqual(ApplyOutcome.applied, holder.apply(base.value));
    try std.testing.expectEqual(ApplyOutcome.unchanged, holder.apply(base.value));

    // Mode alone: read-only → read-write widens the sandbox boundary and nothing
    // else about the policy moves. This must never read as the same policy.
    const remoded = try testValue(a, policyJson("/srv/models", "read_write", "shared model cache"));
    defer remoded.deinit();
    try std.testing.expectEqual(ApplyOutcome.applied, holder.apply(remoded.value));
    try std.testing.expectEqual(ApplyOutcome.unchanged, holder.apply(remoded.value));

    // Path alone.
    const repathed = try testValue(a, policyJson("/srv/other", "read_write", "shared model cache"));
    defer repathed.deinit();
    try std.testing.expectEqual(ApplyOutcome.applied, holder.apply(repathed.value));

    // Note alone. The note is operator prose the self-test echoes back, so a
    // stale one misattributes why a live mount exists.
    const renoted = try testValue(a, policyJson("/srv/other", "read_write", "why it is here"));
    defer renoted.deinit();
    try std.testing.expectEqual(ApplyOutcome.applied, holder.apply(renoted.value));

    // Length alone: dropping the last bind is a real revocation.
    const emptied = try testValue(a,
        \\{"sandbox_tier":"landlock_full","network_policy":"allow_all","registry_allowlist":[],"worker_count":1,"extra_binds":[]}
    );
    defer emptied.deinit();
    try std.testing.expectEqual(ApplyOutcome.applied, holder.apply(emptied.value));
}

test "test_snapshot_deep_copies_the_bind_list" {
    // A worker reads its mounts from the snapshot while the control loop is free
    // to re-apply. Shallow-copied path/note strings would dangle the moment the
    // holder cleared, so the snapshot is read back AFTER the holder is emptied.
    const a = std.testing.allocator;
    var holder = AppliedPolicy.init(a);
    defer holder.deinit();

    const v = try testValue(a, policyJson("/srv/models", "read_write", "shared model cache"));
    defer v.deinit();
    try std.testing.expectEqual(ApplyOutcome.applied, holder.apply(v.value));

    const snap = holder.snapshot(a) orelse return error.TestUnexpectedResult;
    defer freePolicy(a, snap);

    holder.clear();

    try std.testing.expectEqual(@as(usize, 1), snap.extra_binds.len);
    try std.testing.expectEqualStrings("/srv/models", snap.extra_binds[0].path);
    try std.testing.expectEqualStrings("shared model cache", snap.extra_binds[0].note);
    try std.testing.expectEqual(protocol.BindMode.read_write, snap.extra_binds[0].mode);
}

test "test_a_partially_copied_bind_list_is_freed_not_stranded" {
    // Each entry owns TWO strings, so a failure on the note dupe must unwind the
    // path dupe of the same entry as well as both strings of every entry already
    // copied. The control loop snapshots per lease, so a leak here recurs for the
    // life of the daemon rather than once. Two binds put the failure past the
    // first entry; testing.allocator fails the test if anything survives.
    const a = std.testing.allocator;
    var holder = AppliedPolicy.init(a);
    defer holder.deinit();

    const v = try testValue(a,
        \\{"sandbox_tier":"landlock_full","network_policy":"allow_all","registry_allowlist":[],"worker_count":1,
        \\ "extra_binds":[{"path":"/srv/models","mode":"read_only","note":"first"},
        \\                {"path":"/srv/data","mode":"read_write","note":"second"}]}
    );
    defer v.deinit();
    try std.testing.expectEqual(ApplyOutcome.applied, holder.apply(v.value));

    // Walk the allocation index across the whole copy: the allowlist, the bind
    // list, then both strings of each entry. An index that lands ON an
    // allocation must yield null — never a partial policy the caller would
    // lease against. An index past the last one simply succeeds, and is freed
    // here rather than asserted against: the exact allocation count is an
    // implementation detail that moves when the baseline grows, and pinning a
    // number would turn a harmless change into a failing test.
    for (0..12) |fail_index| {
        var fa = std.testing.FailingAllocator.init(a, .{ .fail_index = fail_index });
        if (holder.snapshot(fa.allocator())) |complete| {
            // Reachable only past the last allocation, where nothing failed.
            // `has_induced_failure` is what separates that from a snapshot that
            // survived a refused allocation — which would be the partial policy
            // this test exists to forbid.
            try std.testing.expect(!fa.has_induced_failure);
            freePolicy(fa.allocator(), complete);
        } else {
            try std.testing.expect(fa.has_induced_failure);
        }
    }

    // The holder itself is untouched by a failed snapshot.
    const good = holder.snapshot(a) orelse return error.TestUnexpectedResult;
    defer freePolicy(a, good);
    try std.testing.expectEqual(@as(usize, 2), good.extra_binds.len);
    try std.testing.expectEqualStrings("/srv/data", good.extra_binds[1].path);
}
