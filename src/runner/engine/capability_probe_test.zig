//! Tests for `capability_probe.parseControllers` — the kernel's cgroup
//! controller line turned into owned names.
//!
//! The parser allocates twice per controller (the name, then the list slot) and
//! unwinds through two `errdefer` blocks. Nothing drove either: a failure part
//! way down a controller list leaked every name already copied, once per boot
//! on the hosts where the read is short.

const std = @import("std");
const capability_probe = @import("capability_probe.zig");

/// Free what `parseControllers` returns: the names, then the list holding them.
fn freeControllers(alloc: std.mem.Allocator, names: []const []const u8) void {
    for (names) |n| alloc.free(n);
    alloc.free(names);
}

test "parseControllers splits on every whitespace form the kernel writes" {
    const alloc = std.testing.allocator;
    // The real read is space-separated with a trailing newline; tabs and \r
    // appear on hosts whose cgroup files were written by other tooling.
    const names = try capability_probe.parseControllers(alloc, "cpu\tmemory pids\r\n");
    defer freeControllers(alloc, names);

    try std.testing.expectEqual(@as(usize, 3), names.len);
    try std.testing.expectEqualStrings("cpu", names[0]);
    try std.testing.expectEqualStrings("memory", names[1]);
    try std.testing.expectEqualStrings("pids", names[2]);
}

test "parseControllers yields nothing for an empty or whitespace-only read" {
    const alloc = std.testing.allocator;
    // A host with cgroup v2 mounted but no controllers delegated reads empty.
    // That is not an error — it is a host that can run nothing under a cgroup.
    const empty = try capability_probe.parseControllers(alloc, "");
    defer freeControllers(alloc, empty);
    try std.testing.expectEqual(@as(usize, 0), empty.len);

    const blank = try capability_probe.parseControllers(alloc, "  \n\t ");
    defer freeControllers(alloc, blank);
    try std.testing.expectEqual(@as(usize, 0), blank.len);
}

test "parseControllers frees every name it had copied when an allocation fails" {
    // Walk the whole unwind ladder: fail the Nth allocation for every N until
    // the parse succeeds. Each failing iteration must free the names already
    // duped and the list holding them — `std.testing.allocator` under the
    // FailingAllocator reports any survivor as a leak.
    const text = "cpu memory pids io cpuset";
    var fail_index: usize = 0;
    while (fail_index < 32) : (fail_index += 1) {
        var fa = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = fail_index });
        const alloc = fa.allocator();
        const names = capability_probe.parseControllers(alloc, text) catch |err| {
            try std.testing.expectEqual(error.OutOfMemory, err);
            continue;
        };
        freeControllers(alloc, names);
        return; // the ladder is exhausted — every earlier index unwound cleanly
    }
    return error.TestUnexpectedResult; // never succeeded: not a ladder proof
}
