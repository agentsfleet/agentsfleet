const std = @import("std");
const metrics = @import("../observability/metrics.zig");
const log = std.log.scoped(.worker);

pub const WorkerAllocator = std.heap.GeneralPurposeAllocator(.{});

pub fn finalizeWorkerAllocator(gpa: *WorkerAllocator) bool {
    return switch (gpa.deinit()) {
        .ok => false,
        .leak => blk: {
            metrics.incWorkerAllocatorLeaks();
            log.warn("worker allocator leak detected", .{});
            break :blk true;
        },
    };
}

test "integration: finalizeWorkerAllocator returns false for clean allocator" {
    var gpa = WorkerAllocator{};
    const alloc = gpa.allocator();
    const buf = try alloc.alloc(u8, 32);
    alloc.free(buf);

    try std.testing.expect(!finalizeWorkerAllocator(&gpa));
}

test "integration: finalizeWorkerAllocator returns true when leaks are present" {
    var gpa = WorkerAllocator{};
    const alloc = gpa.allocator();
    const leaked = try alloc.alloc(u8, 32);
    _ = leaked;

    try std.testing.expect(finalizeWorkerAllocator(&gpa));
}
