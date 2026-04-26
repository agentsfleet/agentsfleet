//! Generic free-list object pool with optional thread-safety, capacity bound,
//! and reset hook. Vendored and adapted from
//! https://github.com/oven-sh/bun (`src/pool.zig`, MIT, commit
//! `dc578b12eca413e16b6bbea117ff24b73b48187f`). Bun's pool is a global
//! singleton (threadlocal or static); we re-shape it as a per-instance
//! struct with an explicit `allocator` field, an options struct, and a
//! caller-supplied reset hook so reuse is observable from outside the
//! pooled type. Stripped: `bun.assert`, `bun.ByteList` special-case in
//! `destroyNode`, the JSC log-allocations debug knob.
//!
//! Pool nodes are heap-allocated `T` slots threaded into a singly-linked
//! free list. `acquire` returns the head if present, else allocates a fresh
//! `T` (caller is responsible for the *content* of a freshly-allocated `T`;
//! the pool only owns the slot). `release` runs the reset hook then puts
//! the slot back on the list, freeing it instead if `max_capacity` is set
//! and the list is already full.

const std = @import("std");
const Allocator = std.mem.Allocator;

pub const Options = struct {
    thread_safe: bool = false,
    max_capacity: ?usize = null,
    reset: ?*const fn (*anyopaque) void = null,
};

pub fn ObjectPool(comptime T: type, comptime opts: Options) type {
    return struct {
        const Self = @This();

        const Node = struct {
            next: ?*Node = null,
            data: T,
        };

        const Mutex = if (opts.thread_safe) std.Thread.Mutex else void;

        allocator: Allocator,
        first: ?*Node = null,
        count: usize = 0,
        mu: Mutex = if (opts.thread_safe) .{} else {},

        pub fn init(allocator: Allocator) Self {
            return .{ .allocator = allocator };
        }

        pub fn deinit(self: *Self) void {
            self.lock();
            defer self.unlock();

            var it = self.first;
            self.first = null;
            self.count = 0;
            while (it) |node| {
                it = node.next;
                self.allocator.destroy(node);
            }
        }

        /// Pop the head of the free list, or allocate a fresh slot. Caller
        /// owns `T`'s contents — the pool just owns the slot lifetime.
        pub fn acquire(self: *Self) Allocator.Error!*T {
            self.lock();
            if (self.first) |node| {
                self.first = node.next;
                self.count -|= 1;
                self.unlock();
                if (opts.reset) |reset_fn| reset_fn(@ptrCast(&node.data));
                return &node.data;
            }
            self.unlock();

            const node = try self.allocator.create(Node);
            node.* = .{ .next = null, .data = undefined };
            return &node.data;
        }

        /// Return a slot to the pool. If `max_capacity` is set and the list is
        /// at the cap, the slot is freed instead of retained.
        pub fn release(self: *Self, item: *T) void {
            const node: *Node = @fieldParentPtr("data", item);

            self.lock();
            if (opts.max_capacity) |cap| {
                if (self.count >= cap) {
                    self.unlock();
                    self.allocator.destroy(node);
                    return;
                }
            }
            node.next = self.first;
            self.first = node;
            self.count += 1;
            self.unlock();
        }

        inline fn lock(self: *Self) void {
            if (opts.thread_safe) self.mu.lock();
        }

        inline fn unlock(self: *Self) void {
            if (opts.thread_safe) self.mu.unlock();
        }
    };
}
