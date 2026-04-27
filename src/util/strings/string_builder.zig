//! Capacity-counted string builder. Two-phase: count → allocate → append.
//!
//! Vendored and adapted from https://github.com/oven-sh/bun
//! (`src/string/StringBuilder.zig`, MIT). Stripped: UTF-16 conversion paths
//! (`bun.simdutf`), `bun.String` JS interop, `bun.copy` (we use `@memcpy`),
//! `bun.StringPointer` return type (we return raw `[]const u8`), the
//! `Environment.allow_assert` gating (we use plain `std.debug.assert` —
//! debug-on / release-off automatically per the Zig std lib).
//!
//! Use this when concatenating many slices whose total length is computable
//! up front — calling `count` per slice, then `allocate` once, then `append`
//! per slice avoids the realloc churn of an `ArrayList(u8)`. The returned
//! slices borrow from the builder's backing buffer, so their lifetime is
//! the builder's lifetime; copy out before `deinit` if you need to outlive it.

const std = @import("std");
const Allocator = std.mem.Allocator;

const StringBuilder = @This();

len: usize = 0,
cap: usize = 0,
ptr: ?[*]u8 = null,

/// Single-step alternative to count → allocate when the cap is known up front.
fn initCapacity(alloc: Allocator, cap: usize) Allocator.Error!StringBuilder {
    return .{
        .cap = cap,
        .len = 0,
        .ptr = (try alloc.alloc(u8, cap)).ptr,
    };
}

/// Count phase: add `slice.len` to the running cap. Call before `allocate`.
pub fn count(self: *StringBuilder, slice: []const u8) void {
    self.cap += slice.len;
}

/// Count phase for null-terminated output: reserves `slice.len + 1`.
fn countZ(self: *StringBuilder, slice: []const u8) void {
    self.cap += slice.len + 1;
}

/// Reserves `self.cap` bytes. Call once after all `count` / `countZ` /
/// `fmtCount` calls, before any `append` / `appendZ` / `fmt`.
pub fn allocate(self: *StringBuilder, alloc: Allocator) Allocator.Error!void {
    const slice = try alloc.alloc(u8, self.cap);
    self.ptr = slice.ptr;
    self.len = 0;
}

/// Frees the backing buffer. Idempotent on never-allocated builders.
pub fn deinit(self: *StringBuilder, alloc: Allocator) void {
    if (self.ptr == null or self.cap == 0) return;
    alloc.free(self.ptr.?[0..self.cap]);
    self.ptr = null;
    self.cap = 0;
    self.len = 0;
}

/// Append a slice into the buffer. Returns a slice into the builder's storage —
/// the slice is valid until the builder's `deinit` is called. Asserts
/// (debug-only) that capacity was correctly counted and `allocate` was called.
pub fn append(self: *StringBuilder, slice: []const u8) []const u8 {
    std.debug.assert(self.len <= self.cap);
    std.debug.assert(self.ptr != null);

    @memcpy(self.ptr.?[self.len .. self.len + slice.len], slice);
    const result = self.ptr.?[self.len .. self.len + slice.len];
    self.len += slice.len;

    std.debug.assert(self.len <= self.cap);
    return result;
}

/// Append a null-terminated slice. Reserves `slice.len + 1` (caller used `countZ`).
fn appendZ(self: *StringBuilder, slice: []const u8) [:0]const u8 {
    std.debug.assert(self.len + slice.len + 1 <= self.cap);
    std.debug.assert(self.ptr != null);

    @memcpy(self.ptr.?[self.len .. self.len + slice.len], slice);
    self.ptr.?[self.len + slice.len] = 0;
    const result = self.ptr.?[self.len .. self.len + slice.len :0];
    self.len += slice.len + 1;

    std.debug.assert(self.len <= self.cap);
    return result;
}

/// Append `std.fmt.bufPrint(comptime_fmt, args)` into the builder. The caller
/// must have counted the formatted length via `fmtCount` to ensure capacity.
/// Panics in debug if the formatted output exceeds the remaining capacity.
pub fn fmt(self: *StringBuilder, comptime comptime_fmt: []const u8, args: anytype) []const u8 {
    std.debug.assert(self.len <= self.cap);
    std.debug.assert(self.ptr != null);

    const buf = self.ptr.?[self.len..self.cap];
    const out = std.fmt.bufPrint(buf, comptime_fmt, args) catch unreachable;
    self.len += out.len;

    std.debug.assert(self.len <= self.cap);
    return out;
}

/// Count phase counterpart of `fmt`. Reserves `std.fmt.count(comptime_fmt, args)` bytes.
pub fn fmtCount(self: *StringBuilder, comptime comptime_fmt: []const u8, args: anytype) void {
    self.cap += std.fmt.count(comptime_fmt, args);
}

/// Returns the entire allocated slice (including the unwritten tail). Use
/// when you need raw access (e.g. to fill via `@memcpy` then advance `len`).
fn allocatedSlice(self: *StringBuilder) []u8 {
    const ptr = self.ptr orelse return &.{};
    std.debug.assert(self.cap > 0);
    return ptr[0..self.cap];
}

/// Returns the unwritten remainder of the buffer.
fn writable(self: *StringBuilder) []u8 {
    const ptr = self.ptr orelse return &.{};
    std.debug.assert(self.cap > 0);
    return ptr[self.len..self.cap];
}

/// Transfer ownership of the underlying memory to `into_slice`. After this
/// the builder is empty (zero cap) and must not be `append`-ed to again.
/// Caller is responsible for freeing the returned slice with the same
/// allocator that `allocate` (or `initCapacity`) received.
fn moveToSlice(self: *StringBuilder, into_slice: *[]u8) void {
    into_slice.* = self.allocatedSlice();
    self.* = .{};
}

test "count → allocate → append round-trips a concatenation" {
    const alloc = std.testing.allocator;
    var b: StringBuilder = .{};
    defer b.deinit(alloc);

    b.count("hello, ");
    b.count("world");

    try b.allocate(alloc);
    _ = b.append("hello, ");
    _ = b.append("world");

    try std.testing.expectEqualStrings("hello, world", b.allocatedSlice()[0..b.len]);
}

test "appendZ writes a null terminator" {
    const alloc = std.testing.allocator;
    var b: StringBuilder = .{};
    defer b.deinit(alloc);

    b.countZ("name");
    try b.allocate(alloc);
    const out = b.appendZ("name");
    try std.testing.expectEqualStrings("name", out);
    try std.testing.expectEqual(@as(u8, 0), b.allocatedSlice()[4]);
}

test "fmt + fmtCount work together" {
    const alloc = std.testing.allocator;
    var b: StringBuilder = .{};
    defer b.deinit(alloc);

    b.fmtCount("user={s} id={d}", .{ "alice", 42 });
    try b.allocate(alloc);
    const out = b.fmt("user={s} id={d}", .{ "alice", 42 });
    try std.testing.expectEqualStrings("user=alice id=42", out);
}

test "initCapacity skips the count phase" {
    const alloc = std.testing.allocator;
    var b = try StringBuilder.initCapacity(alloc, 32);
    defer b.deinit(alloc);

    _ = b.append("preallocated");
    try std.testing.expectEqualStrings("preallocated", b.allocatedSlice()[0..b.len]);
}
