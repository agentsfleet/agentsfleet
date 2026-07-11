//! Fault injection for testing error-handling paths.
//!
//! Vendored from ghostty (`src/tripwire.zig`, MIT, mitchellh/ghostty @ 8cddd384)
//! with the API kept intact so upstream fixes port over mechanically.
//!
//! Improper `errdefer` is one of the highest sources of bugs in Zig code.
//! Many `errdefer` points are hard to exercise in unit tests and rare to
//! encounter in production, so they often hide bugs. Worse, error scenarios
//! are most likely to put code in an unexpected state that surfaces later as
//! assertion failures or memory-safety issues.
//!
//! This module injects errors at named points during unit tests so every
//! error path is testable. Outside test builds it is comptime-erased: zero
//! binary size, zero runtime cost, every `check` call optimized away.
//!
//! # Usage
//!
//! Create one `tripwire.module` per fallible function under test. The enum is
//! the hand-curated set of fail points; the error set comes from the function
//! itself (or use a narrow explicit set).
//!
//! Place `try tw.check(.point)` directly before each `try` whose errdefer
//! ladder you want to exercise — not every `try` needs one, only the ones
//! whose cleanup you care to prove.
//!
//! In tests, arm points with `errorAlways`/`errorAfter`, call the function,
//! then ALWAYS `try tw.end(.reset)` — it fails the test if an armed point
//! never tripped and resets state for the next test.
//!
//! ```
//! const tw = tripwire.module(enum { alloc_buf, open_file }, error{OutOfMemory});
//!
//! fn myFunction() !void {
//!     try tw.check(.alloc_buf);
//!     const buf = try allocator.alloc(u8, 1024);
//!     errdefer allocator.free(buf);
//!     // ...
//! }
//!
//! test "myFunction frees buf when the later step fails" {
//!     tw.errorAlways(.open_file, error.OutOfMemory);
//!     try std.testing.expectError(error.OutOfMemory, myFunction());
//!     try tw.end(.reset);
//! }
//! ```
//!
//! The canonical leak proof loops every fail point under
//! `std.testing.allocator` (which fails the test on any leak):
//!
//! ```
//! for (std.meta.tags(tw.FailPoint)) |tag| {
//!     defer tw.end(.reset) catch unreachable;
//!     tw.errorAlways(tag, error.OutOfMemory);
//!     try std.testing.expectError(error.OutOfMemory, myFunction());
//! }
//! ```
//!
//! For transitive calls, either put a fail point above the call in the caller
//! (trusting the child's own tests), or give the child its own module — the
//! latter when the child can't be exercised in isolation.

const std = @import("std");
const builtin = @import("builtin");
const assert = std.debug.assert;
const testing = std.testing;

// Test-only diagnostic channel: the sole emit (`untripped_point`) is reachable
// only when `enabled` (== builtin.is_test) — the structured `log` named module
// is deliberately not imported so this file keeps zero named-module deps and
// stays eligible for the src/lib/tests.zig aggregator root.
const log = std.log.scoped(.tripwire);

/// A tripwire module injecting failures at named points.
///
/// P is an enum of the failure points. E is the error set the points may
/// return: an error set, an error-union type, or a function (whose return
/// error set is used). `anyerror` works but may need `checkConstrained` at
/// call sites so the enclosing function's error set stays narrow.
pub fn module(
    comptime P: type,
    comptime E: anytype,
) type {
    return struct {
        /// The points this module can fail at.
        pub const FailPoint = P;

        /// The error set used for failures at the failure points.
        pub const Error = err: {
            const T = if (@TypeOf(E) == type) E else @TypeOf(E);
            break :err switch (@typeInfo(T)) {
                .error_set => E,
                .error_union => |info| info.error_set,
                .@"fn" => |info| @typeInfo(info.return_type.?).error_union.error_set,
                else => @compileError("E must be an error set or function type"),
            };
        };

        /// Comptime-erased outside test builds: checks compile to nothing.
        pub const enabled = builtin.is_test;

        comptime {
            assert(@typeInfo(FailPoint) == .@"enum");
            assert(@typeInfo(Error) == .error_set);
        }

        /// The armed tripwires for this module (test-global by design: one
        /// module const per function under test, reset via `end`).
        var tripwires: TripwireMap = .{};
        const TripwireMap = std.EnumMap(FailPoint, Tripwire);
        const Tripwire = struct {
            /// Error to return when tripped.
            err: Error,

            /// Times this point has been reached — NOT times tripped
            /// (`min` gates tripping).
            reached: usize = 0,

            /// Minimum reaches before tripping; after that it trips every
            /// time. 0 = trip on first reach.
            min: usize = 0,

            /// True once this point has tripped at least once.
            tripped: bool = false,
        };

        /// Check for a failure at the given point. Place directly before the
        /// `try` operation whose cleanup path is under test.
        pub fn check(point: FailPoint) callconv(callingConvention()) Error!void {
            if (comptime !enabled) return;
            return checkConstrained(point, Error);
        }

        /// Same as `check` with a narrower error type for the return value —
        /// must be a subset of `Error`; runtime error if the armed error
        /// cannot be represented in `ConstrainedError`.
        pub fn checkConstrained(
            point: FailPoint,
            comptime ConstrainedError: type,
        ) callconv(callingConvention()) ConstrainedError!void {
            if (comptime !enabled) return;
            const tripwire = tripwires.getPtr(point) orelse return;
            tripwire.reached += 1;
            if (tripwire.reached <= tripwire.min) return;
            tripwire.tripped = true;
            return tripwire.err;
        }

        /// Arm a point to trip with `err` on every reach.
        pub fn errorAlways(point: FailPoint, err: Error) void {
            errorAfter(point, err, 0);
        }

        /// Arm a point to trip with `err` after it has been reached `min`
        /// times (0 == `errorAlways`).
        pub fn errorAfter(point: FailPoint, err: Error, min: usize) void {
            tripwires.put(point, .{ .err = err, .min = min });
        }

        /// End the tripwire session: errors if any armed expectation never
        /// tripped. Expectations are cleared per `reset_mode` (always cleared
        /// on the error path too, so one failing test cannot poison the next).
        pub fn end(reset_mode: enum { reset, retain }) error{UntrippedError}!void {
            var untripped: bool = false;
            var iter = tripwires.iterator();
            while (iter.next()) |entry| {
                if (!entry.value.tripped) {
                    log.warn("untripped_point point={s}", .{@tagName(entry.key)});
                    untripped = true;
                }
            }

            switch (reset_mode) {
                .reset => reset(),
                .retain => {},
            }

            if (untripped) return error.UntrippedError;
        }

        /// Unset all tripwires. Prefer `end(.reset)` — it also verifies
        /// expectations.
        pub fn reset() void {
            tripwires = .{};
        }

        /// Inline when disabled so every `check` call is optimized away.
        fn callingConvention() std.builtin.CallingConvention {
            return if (!enabled) .@"inline" else .auto;
        }
    };
}

test {
    const io = module(enum {
        read,
        write,
    }, anyerror);

    // Reset should work.
    try io.end(.reset);

    // By default, pass-through.
    try io.check(.read);

    // Always trip.
    io.errorAlways(.read, error.OutOfMemory);
    try testing.expectError(
        error.OutOfMemory,
        io.check(.read),
    );
    // Happens again.
    try testing.expectError(
        error.OutOfMemory,
        io.check(.read),
    );
    try io.end(.reset);
}

test "module as error set" {
    const io = module(enum { read, write }, @TypeOf((struct {
        fn func() error{ Foo, Bar }!void {
            return error.Foo;
        }
    }).func));
    try io.end(.reset);
}

test "errorAfter" {
    const io = module(enum { read, write }, anyerror);
    // Trip after 2 calls (on the 3rd call).
    io.errorAfter(.read, error.OutOfMemory, 2);

    // First two calls succeed.
    try io.check(.read);
    try io.check(.read);

    // Third call and on trips.
    try testing.expectError(error.OutOfMemory, io.check(.read));
    try testing.expectError(error.OutOfMemory, io.check(.read));

    try io.end(.reset);
}

test "errorAfter untripped error if min not reached" {
    // The untripped path warns by design; this test EXPECTS it, and the build
    // runner fails a passing test that emits warn+ logs — silence it here only.
    const saved_log_level = testing.log_level;
    testing.log_level = .err;
    defer testing.log_level = saved_log_level;

    const io = module(enum { read }, anyerror);
    io.errorAfter(.read, error.OutOfMemory, 2);
    // Only one reach — not enough to trip.
    try io.check(.read);
    // end fails: armed but never tripped.
    try testing.expectError(
        error.UntrippedError,
        io.end(.reset),
    );
}

test "check on an unarmed point counts nothing and passes" {
    const io = module(enum { read, write }, anyerror);
    io.errorAlways(.write, error.AccessDenied);
    // Unarmed point never trips.
    try io.check(.read);
    // Armed point trips with its exact error.
    try testing.expectError(error.AccessDenied, io.check(.write));
    try io.end(.reset);
}
