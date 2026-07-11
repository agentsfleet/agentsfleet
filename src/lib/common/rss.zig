//! Current resident-set-size (RSS) reader — the process-level memory oracle the
//! in-process `std.testing.allocator` cannot see. `testing.allocator` proves the
//! Zig-heap paths leak-free, but growth in `c_allocator`, `page_allocator`, or a
//! native library (openssl, sqlite, wasm3) is invisible to it, and Valgrind
//! cannot run on the macOS dev machines. The RSS-growth soak probes read this
//! (baseline -> workload xN -> assert bounded growth) to bound that layer.
//!
//! Returns `null` where no reader exists so a probe *skips* rather than
//! false-fails. Lives in `common` (pure, datastore-free): the Linux path is a
//! raw-syscall `/proc` read (no libc, no `io`) and the macOS path is a mach
//! `task_info` call (libSystem, always linked on Darwin). No logging dependency,
//! so no `common -> log` import cycle.

const std = @import("std");
const builtin = @import("builtin");

/// Current resident set size of THIS process, in bytes. `null` on a platform
/// without a reader (a probe treats null as "skip", never a failure) or on a
/// transient read/parse error.
pub fn currentBytes() ?u64 {
    return switch (builtin.os.tag) {
        .linux => linuxCurrentBytes(),
        .macos => macosCurrentBytes(),
        else => null,
    };
}

/// Linux: `/proc/self/statm` is space-separated page counts
/// ("size resident shared text lib data dt"). Field index 1 is resident pages;
/// RSS = resident_pages * runtime page size. Read via raw syscalls (no libc, no
/// `io`) so `common` stays dependency-free.
fn linuxCurrentBytes() ?u64 {
    const fd = std.posix.openatZ(std.posix.AT.FDCWD, "/proc/self/statm", .{ .ACCMODE = .RDONLY }, 0) catch return null;
    defer std.posix.close(fd);
    var buf: [128]u8 = undefined;
    const n = std.posix.read(fd, &buf) catch return null;
    if (n == 0) return null;
    var it = std.mem.tokenizeScalar(u8, buf[0..n], ' ');
    _ = it.next() orelse return null; // field 0: total program size (skip)
    const resident = it.next() orelse return null; // field 1: resident pages
    const pages = std.fmt.parseInt(u64, std.mem.trim(u8, resident, " \n\r\t"), 10) catch return null;
    return pages * @as(u64, std.heap.pageSize());
}

/// macOS: mach `task_info(MACH_TASK_BASIC_INFO)` — `resident_size` is the current
/// resident bytes. `std.c.MACH_TASK_BASIC_INFO` is a placeholder that redirects
/// to a nested namespace, so the flavor value (20) is pinned here as a named
/// constant. libSystem is always linked on Darwin, so the externs resolve even
/// in the otherwise libc-free lib test graph.
fn macosCurrentBytes() ?u64 {
    const MACH_TASK_BASIC_INFO: std.c.task_flavor_t = 20;
    // SAFETY: task_info fully writes info on success; resident_size is read only
    // after the kr == 0 (KERN_SUCCESS) guard below, never while unpopulated.
    var info: std.c.mach_task_basic_info = undefined;
    var count: std.c.mach_msg_type_number_t = @sizeOf(std.c.mach_task_basic_info) / @sizeOf(u32);
    const kr = std.c.task_info(std.c.mach_task_self(), MACH_TASK_BASIC_INFO, @ptrCast(&info), &count);
    if (kr != 0) return null; // non-zero == not KERN_SUCCESS
    return info.resident_size;
}

const MIN_PLAUSIBLE_RSS_BYTES: u64 = 1024 * 1024; // a live test process is always > 1 MiB resident

test "currentBytes: non-null and plausibly-sized on this platform" {
    const rss = currentBytes();
    switch (builtin.os.tag) {
        .linux, .macos => {
            try std.testing.expect(rss != null);
            try std.testing.expect(rss.? > MIN_PLAUSIBLE_RSS_BYTES);
        },
        else => try std.testing.expect(rss == null), // unsupported platform -> skip signal
    }
}
