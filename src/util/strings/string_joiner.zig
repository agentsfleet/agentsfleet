//! Rope-like data structure for joining many small slices into one big string.
//!
//! Vendored and adapted from https://github.com/oven-sh/bun
//! (`src/string/StringJoiner.zig`, MIT). Stripped: `bun.handleOom` (we
//! propagate `OutOfMemory` errors), `NullableAllocator` (we use `?Allocator`),
//! `bun.strings.contains` (we use `std.mem.indexOf`), the watcher
//! `estimated_count` heuristic (Bun-specific bundler use case).
//!
//! Use this when assembling many small slices whose total length is not
//! known in advance — `pushStatic` for borrowed slices, `pushCloned` for
//! caller-owned data we want to outlive the source. `done(alloc)` returns
//! the joined string owned by `alloc` and frees the joiner's internal nodes.

const std = @import("std");
const Allocator = std.mem.Allocator;

const StringJoiner = @This();

allocator: Allocator,
len: usize = 0,
head: ?*Node = null,
tail: ?*Node = null,

const Node = struct {
    /// Per-slice allocator. Null = static / borrowed slice that the joiner
    /// must NOT free. Set = the joiner duped the slice into this allocator
    /// and is responsible for freeing it.
    slice_allocator: ?Allocator = null,
    slice: []const u8 = "",
    next: ?*Node = null,

    fn init(joiner_alloc: Allocator, slice: []const u8, slice_alloc: ?Allocator) Allocator.Error!*Node {
        const node = try joiner_alloc.create(Node);
        node.* = .{
            .slice = slice,
            .slice_allocator = slice_alloc,
        };
        return node;
    }

    fn deinit(self: *Node, joiner_alloc: Allocator) void {
        if (self.slice_allocator) |a| a.free(self.slice);
        joiner_alloc.destroy(self);
    }
};

pub fn init(alloc: Allocator) StringJoiner {
    return .{ .allocator = alloc };
}

/// Push a borrowed slice. Caller must keep `data` alive until `done` is called.
pub fn pushStatic(self: *StringJoiner, data: []const u8) Allocator.Error!void {
    try self.push(data, null);
}

/// Push a duplicated copy of `data`. The joiner owns the copy.
fn pushCloned(self: *StringJoiner, data: []const u8) Allocator.Error!void {
    if (data.len == 0) return;
    const owned = try self.allocator.dupe(u8, data);
    errdefer self.allocator.free(owned);
    try self.push(owned, self.allocator);
}

fn push(self: *StringJoiner, data: []const u8, slice_alloc: ?Allocator) Allocator.Error!void {
    if (data.len == 0) return;
    self.len += data.len;

    const new_tail = try Node.init(self.allocator, data, slice_alloc);

    if (self.tail) |current_tail| {
        current_tail.next = new_tail;
    } else {
        std.debug.assert(self.head == null);
        self.head = new_tail;
    }
    self.tail = new_tail;
}

/// Concatenate everything and return the result owned by `out_alloc`. Frees
/// the joiner's internal nodes (and any cloned slices). The joiner is empty
/// after `done` returns and may be reused or `deinit`-ed.
pub fn done(self: *StringJoiner, out_alloc: Allocator) Allocator.Error![]u8 {
    const out = try out_alloc.alloc(u8, self.len);
    errdefer out_alloc.free(out);

    var remaining = out;
    var current: ?*Node = self.head;
    while (current) |node| {
        @memcpy(remaining[0..node.slice.len], node.slice);
        remaining = remaining[node.slice.len..];
        current = node.next;
        node.deinit(self.allocator);
    }
    std.debug.assert(remaining.len == 0);

    self.head = null;
    self.tail = null;
    self.len = 0;
    return out;
}

/// Same as `done`, but appends `end` after the joined nodes.
fn doneWithEnd(self: *StringJoiner, out_alloc: Allocator, end: []const u8) Allocator.Error![]u8 {
    const out = try out_alloc.alloc(u8, self.len + end.len);
    errdefer out_alloc.free(out);

    var remaining = out;
    var current: ?*Node = self.head;
    while (current) |node| {
        @memcpy(remaining[0..node.slice.len], node.slice);
        remaining = remaining[node.slice.len..];
        current = node.next;
        node.deinit(self.allocator);
    }
    @memcpy(remaining[0..end.len], end);

    self.head = null;
    self.tail = null;
    self.len = 0;
    return out;
}

/// Discard everything without producing output. Frees nodes + cloned slices.
pub fn deinit(self: *StringJoiner) void {
    var current: ?*Node = self.head;
    while (current) |node| {
        const next = node.next;
        node.deinit(self.allocator);
        current = next;
    }
    self.head = null;
    self.tail = null;
    self.len = 0;
}

fn lastByte(self: *const StringJoiner) u8 {
    const slice = (self.tail orelse return 0).slice;
    std.debug.assert(slice.len > 0);
    return slice[slice.len - 1];
}

pub fn contains(self: *const StringJoiner, needle: []const u8) bool {
    var current = self.head;
    while (current) |node| {
        current = node.next;
        if (std.mem.indexOf(u8, node.slice, needle) != null) return true;
    }
    return false;
}

test "pushStatic + done returns concatenation" {
    const alloc = std.testing.allocator;
    var j = StringJoiner.init(alloc);

    try j.pushStatic("hello, ");
    try j.pushStatic("world");

    const out = try j.done(alloc);
    defer alloc.free(out);
    try std.testing.expectEqualStrings("hello, world", out);
}

test "pushCloned outlives the source slice" {
    const alloc = std.testing.allocator;
    var j = StringJoiner.init(alloc);

    {
        var tmp_buf: [16]u8 = undefined;
        const tmp = try std.fmt.bufPrint(&tmp_buf, "tempval", .{});
        try j.pushCloned(tmp);
    }

    const out = try j.done(alloc);
    defer alloc.free(out);
    try std.testing.expectEqualStrings("tempval", out);
}

test "doneWithEnd appends a trailing slice" {
    const alloc = std.testing.allocator;
    var j = StringJoiner.init(alloc);
    try j.pushStatic("a");
    try j.pushStatic("b");

    const out = try j.doneWithEnd(alloc, "!\n");
    defer alloc.free(out);
    try std.testing.expectEqualStrings("ab!\n", out);
}

test "deinit frees nodes without producing output" {
    const alloc = std.testing.allocator;
    var j = StringJoiner.init(alloc);
    try j.pushCloned("data1");
    try j.pushStatic("data2");
    j.deinit();
}

test "contains finds a needle across nodes" {
    const alloc = std.testing.allocator;
    var j = StringJoiner.init(alloc);
    defer j.deinit();
    try j.pushStatic("hello");
    try j.pushStatic("world");

    try std.testing.expect(j.contains("ello"));
    try std.testing.expect(!j.contains("missing"));
}

test "empty joiner returns empty string" {
    const alloc = std.testing.allocator;
    var j = StringJoiner.init(alloc);
    const out = try j.done(alloc);
    defer alloc.free(out);
    try std.testing.expectEqual(@as(usize, 0), out.len);
}
