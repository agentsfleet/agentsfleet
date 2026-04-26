//! Inline-string optimization: stores up to 15 bytes inline, falls back to
//! the heap for longer strings. Packed `u128` representation — the high bit
//! of the pointer doubles as the "inlined" tag.
//!
//! Vendored and adapted from https://github.com/oven-sh/bun
//! (`src/string/SmolStr.zig`, MIT). Stripped: `bun.collections.BabyList`
//! (we use direct `Allocator.alloc` / `realloc`); `jsonStringify` (depends
//! on Bun's JS bridge); `bun.assert` (we use `std.debug.assert`).
//!
//! **Endian assumption**: requires little-endian, which both of usezombie's
//! deploy targets (x86_64-linux, aarch64-linux) satisfy. The compile-time
//! check below makes that explicit.
//!
//! When to reach for this: a struct field that holds many short strings
//! (UUIDs are 36 chars — just over the 15-byte inline cap, so they always
//! heap; tags / labels / status names usually inline). Compared to a bare
//! `[]u8` plus separate cap field, `SmolStr` saves an allocation for short
//! strings and matches std.ArrayList's interface for long ones.

const std = @import("std");
const Allocator = std.mem.Allocator;
const builtin = @import("builtin");

comptime {
    // The packed-struct tag-bit trick relies on little-endian byte order.
    if (builtin.cpu.arch.endian() != .little) {
        @compileError("SmolStr requires a little-endian target");
    }
}

pub const SmolStr = packed struct(u128) {
    __len: u32,
    cap: u32,
    __ptr: [*]u8,

    /// High bit of `__ptr` indicates inline storage. Cleared bits give the
    /// real pointer value.
    const Tag: usize = 0x8000000000000000;
    const NegatedTag: usize = ~Tag;

    pub const Inlined = packed struct(u128) {
        data: u120,
        __len: u7,
        _tag: u1,

        pub const max_len: comptime_int = @bitSizeOf(@FieldType(Inlined, "data")) / 8;

        pub const empty: Inlined = .{ .data = 0, .__len = 0, ._tag = 1 };

        /// Returns error.StringTooLong if `str.len > max_len`.
        pub fn init(str: []const u8) error{StringTooLong}!Inlined {
            if (str.len > max_len) return error.StringTooLong;
            var inlined = Inlined.empty;
            if (str.len > 0) {
                @memcpy(inlined.allChars()[0..str.len], str[0..str.len]);
                inlined.setLen(@intCast(str.len));
            }
            return inlined;
        }

        pub inline fn len(self: Inlined) u8 {
            return @intCast(self.__len);
        }

        pub fn setLen(self: *Inlined, new_len: u7) void {
            self.__len = new_len;
        }

        pub fn slice(self: *const Inlined) []const u8 {
            return @constCast(self).ptr()[0..self.__len];
        }

        pub fn allChars(self: *Inlined) *[max_len]u8 {
            return self.ptr()[0..max_len];
        }

        inline fn ptr(self: *Inlined) [*]u8 {
            return @as([*]u8, @ptrCast(@as(*u128, @ptrCast(self))));
        }
    };

    comptime {
        std.debug.assert(@sizeOf(SmolStr) == @sizeOf(Inlined));
    }

    pub fn empty() SmolStr {
        return SmolStr.fromInlined(Inlined.empty);
    }

    pub fn len(self: *const SmolStr) u32 {
        if (self.isInlined()) {
            return @intCast((@intFromPtr(self.__ptr) >> 56) & 0b01111111);
        }
        return self.__len;
    }

    pub fn ptr(self: *SmolStr) [*]u8 {
        return @ptrFromInt(@as(usize, @intFromPtr(self.__ptr)) & NegatedTag);
    }

    fn ptrConst(self: *const SmolStr) [*]const u8 {
        return @ptrFromInt(@as(usize, @intFromPtr(self.__ptr)) & NegatedTag);
    }

    pub fn markInlined(self: *SmolStr) void {
        self.__ptr = @ptrFromInt(@as(usize, @intFromPtr(self.__ptr)) | Tag);
    }

    pub fn markHeap(self: *SmolStr) void {
        self.__ptr = @ptrFromInt(@as(usize, @intFromPtr(self.__ptr)) & NegatedTag);
    }

    pub fn isInlined(self: *const SmolStr) bool {
        return @as(usize, @intFromPtr(self.__ptr)) & Tag != 0;
    }

    /// Panics in debug if `self` is too long to fit inline.
    pub fn toInlined(self: *const SmolStr) Inlined {
        std.debug.assert(self.len() <= Inlined.max_len);
        var inlined: Inlined = @bitCast(@as(u128, @bitCast(self.*)));
        inlined._tag = 1;
        return inlined;
    }

    pub fn fromInlined(inlined: Inlined) SmolStr {
        var s: SmolStr = @bitCast(inlined);
        s.markInlined();
        return s;
    }

    pub fn fromChar(char: u8) SmolStr {
        var inlined: Inlined = .{ .data = 0, .__len = 1, ._tag = 1 };
        inlined.allChars()[0] = char;
        return SmolStr.fromInlined(inlined);
    }

    /// Caller passes the same allocator to deinit that fromSlice received.
    pub fn fromSlice(alloc: Allocator, values: []const u8) Allocator.Error!SmolStr {
        if (values.len > Inlined.max_len) {
            const buf = try alloc.alloc(u8, values.len);
            @memcpy(buf[0..values.len], values);
            var s: SmolStr = .{
                .__len = @intCast(values.len),
                .cap = @intCast(values.len),
                .__ptr = buf.ptr,
            };
            s.markHeap();
            return s;
        }

        const inlined = Inlined.init(values) catch unreachable;
        return SmolStr.fromInlined(inlined);
    }

    pub fn deinit(self: *SmolStr, alloc: Allocator) void {
        if (!self.isInlined() and self.cap > 0) {
            alloc.free(self.ptrConst()[0..self.cap]);
        }
    }

    pub fn slice(self: *const SmolStr) []const u8 {
        if (self.isInlined()) {
            const bytes: [*]const u8 = @ptrCast(self);
            return bytes[0..self.len()];
        }
        return self.ptrConst()[0..self.__len];
    }

    /// Append `values`, transitioning to heap if needed. Caller must pass
    /// the same allocator used by `fromSlice`.
    pub fn appendSlice(self: *SmolStr, alloc: Allocator, values: []const u8) Allocator.Error!void {
        if (self.isInlined()) {
            var inlined = self.toInlined();
            const new_len = inlined.len() + values.len;
            if (new_len <= Inlined.max_len) {
                @memcpy(inlined.allChars()[inlined.len()..new_len], values);
                inlined.setLen(@intCast(new_len));
                self.* = SmolStr.fromInlined(inlined);
                return;
            }
            const buf = try alloc.alloc(u8, new_len);
            @memcpy(buf[0..inlined.len()], inlined.slice());
            @memcpy(buf[inlined.len()..new_len], values);
            self.* = .{
                .__len = @intCast(new_len),
                .cap = @intCast(new_len),
                .__ptr = buf.ptr,
            };
            self.markHeap();
            return;
        }

        const new_len = self.__len + values.len;
        if (new_len <= self.cap) {
            const p: [*]u8 = @ptrFromInt(@as(usize, @intFromPtr(self.__ptr)) & NegatedTag);
            @memcpy(p[self.__len..new_len], values);
            self.__len = @intCast(new_len);
            return;
        }

        const old_p: [*]u8 = @ptrFromInt(@as(usize, @intFromPtr(self.__ptr)) & NegatedTag);
        const new_cap = @max(new_len, self.cap * 2);
        const new_buf = try alloc.alloc(u8, new_cap);
        @memcpy(new_buf[0..self.__len], old_p[0..self.__len]);
        @memcpy(new_buf[self.__len..new_len], values);
        alloc.free(old_p[0..self.cap]);
        self.__len = @intCast(new_len);
        self.cap = @intCast(new_cap);
        self.__ptr = new_buf.ptr;
        self.markHeap();
    }
};

test "small strings stay inline (no allocation)" {
    var s = try SmolStr.fromSlice(std.testing.allocator, "hello");
    // No defer deinit — fromSlice must not allocate for inlined strings.
    try std.testing.expectEqual(@as(u32, 5), s.len());
    try std.testing.expect(s.isInlined());
    try std.testing.expectEqualStrings("hello", s.slice());
}

test "long strings spill to heap" {
    const alloc = std.testing.allocator;
    var s = try SmolStr.fromSlice(alloc, "this string is longer than fifteen bytes");
    defer s.deinit(alloc);
    try std.testing.expect(!s.isInlined());
    try std.testing.expectEqualStrings("this string is longer than fifteen bytes", s.slice());
}

test "appendSlice transitions inline → heap" {
    const alloc = std.testing.allocator;
    var s = try SmolStr.fromSlice(alloc, "hello");
    defer s.deinit(alloc);
    try std.testing.expect(s.isInlined());

    try s.appendSlice(alloc, " world, this overflows the inline cap");
    try std.testing.expect(!s.isInlined());
    try std.testing.expectEqualStrings("hello world, this overflows the inline cap", s.slice());
}

test "appendSlice grows on heap (within cap)" {
    const alloc = std.testing.allocator;
    var s = try SmolStr.fromSlice(alloc, "this is a long initial string");
    defer s.deinit(alloc);
    try std.testing.expect(!s.isInlined());

    try s.appendSlice(alloc, "+more");
    try std.testing.expectEqualStrings("this is a long initial string+more", s.slice());
}

test "Inlined.init returns StringTooLong on overflow" {
    try std.testing.expectError(error.StringTooLong, SmolStr.Inlined.init("more than fifteen bytes here"));
}

test "fromChar produces a 1-byte inline string" {
    const s = SmolStr.fromChar('x');
    try std.testing.expect(s.isInlined());
    try std.testing.expectEqual(@as(u32, 1), s.len());
    try std.testing.expectEqualStrings("x", s.slice());
}

test "empty() returns a zero-length inlined string" {
    const s = SmolStr.empty();
    try std.testing.expect(s.isInlined());
    try std.testing.expectEqual(@as(u32, 0), s.len());
}
