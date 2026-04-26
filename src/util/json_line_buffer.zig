//! Newline-delimited buffer with one-pass scan + lazy compaction. Each byte
//! is scanned exactly once; consumed lines advance a head offset rather than
//! memmoving the backing buffer; compaction only fires past a 16 MiB threshold.
//!
//! Vendored and adapted from https://github.com/oven-sh/bun
//! (`src/bun.js/JSONLineBuffer.zig`, MIT, commit
//! `dc578b12eca413e16b6bbea117ff24b73b48187f`). Stripped: `bun.ByteList`
//! → `std.ArrayListUnmanaged(u8)`, `bun.default_allocator` → caller-supplied
//! allocator, `bun.strings.indexOfChar` → `std.mem.indexOfScalar`,
//! `bun.handleOom` (Bun aborts on OOM via a global hook) → `!void` returns,
//! `bun.debugAssert` → `std.debug.assert`.
//!
//! API contract: `write` appends bytes (may allocate); `nextLine` returns the
//! next complete line *minus* the trailing `\n` and advances head past it.
//! Returned slice borrows from the backing buffer and is invalidated by the
//! next `write` (or by another `nextLine` call that triggers compaction).

const std = @import("std");
const Allocator = std.mem.Allocator;

const JsonLineBuffer = @This();

allocator: Allocator,
data: std.ArrayListUnmanaged(u8) = .{},
head: u32 = 0,
newline_pos: ?u32 = null,
scanned_pos: u32 = 0,

/// Compact the buffer when head exceeds this threshold (16 MiB).
const compaction_threshold: u32 = 16 * 1024 * 1024;

pub fn init(allocator: Allocator) JsonLineBuffer {
    return .{ .allocator = allocator };
}

pub fn deinit(self: *JsonLineBuffer) void {
    self.data.deinit(self.allocator);
    self.* = .{ .allocator = self.allocator };
}

/// Append bytes; scans only the new tail for `\n`.
pub fn write(self: *JsonLineBuffer, bytes: []const u8) Allocator.Error!void {
    try self.data.appendSlice(self.allocator, bytes);
    self.scanForNewline();
}

/// Return next complete line (without trailing newline) and advance past it.
/// Returns null if no full line is buffered.
pub fn nextLine(self: *JsonLineBuffer) ?[]const u8 {
    const pos = self.newline_pos orelse return null;
    const slice = self.activeSlice();
    const line = slice[0..pos];
    self.consume(pos + 1);
    return line;
}

pub fn isEmpty(self: *const JsonLineBuffer) bool {
    return self.head >= self.data.items.len;
}

fn activeSlice(self: *const JsonLineBuffer) []const u8 {
    return self.data.items[self.head..];
}

fn scanForNewline(self: *JsonLineBuffer) void {
    if (self.newline_pos != null) return;
    const slice = self.activeSlice();
    if (self.scanned_pos >= slice.len) return;

    const unscanned = slice[self.scanned_pos..];
    if (std.mem.indexOfScalar(u8, unscanned, '\n')) |local_idx| {
        std.debug.assert(local_idx <= std.math.maxInt(u32));
        const pos = self.scanned_pos +| @as(u32, @intCast(local_idx));
        self.newline_pos = pos;
        self.scanned_pos = pos +| 1;
    } else {
        std.debug.assert(slice.len <= std.math.maxInt(u32));
        self.scanned_pos = @intCast(slice.len);
    }
}

fn consume(self: *JsonLineBuffer, bytes: u32) void {
    self.head +|= bytes;
    self.scanned_pos = if (bytes >= self.scanned_pos) 0 else self.scanned_pos - bytes;

    if (self.newline_pos) |pos| {
        if (bytes > pos) {
            self.newline_pos = null;
            self.scanForNewline();
        } else {
            self.newline_pos = pos - bytes;
        }
    }

    if (self.head >= self.data.items.len) {
        if (self.data.capacity >= compaction_threshold) {
            self.data.deinit(self.allocator);
            self.data = .{};
        } else {
            self.data.clearRetainingCapacity();
        }
        self.head = 0;
        self.scanned_pos = 0;
        self.newline_pos = null;
        return;
    }

    if (self.head >= compaction_threshold) {
        self.compact();
    }
}

fn compact(self: *JsonLineBuffer) void {
    if (self.head == 0) return;
    const slice = self.activeSlice();
    std.mem.copyForwards(u8, self.data.items[0..slice.len], slice);
    self.data.items.len = slice.len;
    self.head = 0;
}
