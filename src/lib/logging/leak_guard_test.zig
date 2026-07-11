//! Unit tests for the teardown leak-verdict guard (`leak_guard.zig`).
//!
//! `std.heap.Check` is a plain enum, so these pass `.leak`/`.ok` directly rather
//! than seeding a real `DebugAllocator` leak — a real leak makes `deinit()` emit
//! its own `err` log, which the test harness counts as a failure. The `.leak`
//! path through `check` is asserted via a `BufferedSink` capture (test builds
//! route emits through the sink registry, never `std.log`).

const std = @import("std");
const builtin = @import("builtin");
const logging = @import("mod.zig");
const leak_guard = logging.leak_guard;
const sinks = @import("sinks.zig");

test "verdictToError: .ok always passes" {
    try leak_guard.verdictToError(.ok);
}

test "verdictToError: .leak fails in Debug builds, passes elsewhere" {
    if (builtin.mode == .Debug) {
        try std.testing.expectError(error.LeakDetected, leak_guard.verdictToError(.leak));
    } else {
        try leak_guard.verdictToError(.leak); // release: logged at the site, never a hard failure
    }
}

test "check: .leak logs gpa_leak_verdict with the component and returns the verdict" {
    var bs = sinks.BufferedSink.init(std.testing.allocator);
    defer bs.deinit();
    sinks.clearSinksForTest();
    defer sinks.clearSinksForTest();
    sinks.registerSink(bs.sink());

    const result = leak_guard.check(.leak, "daemon");
    if (builtin.mode == .Debug) {
        try std.testing.expectError(error.LeakDetected, result);
    } else {
        try result;
    }

    const captured = try bs.snapshot();
    defer std.testing.allocator.free(captured);
    try std.testing.expect(std.mem.indexOf(u8, captured, "event=gpa_leak_verdict") != null);
    try std.testing.expect(std.mem.indexOf(u8, captured, "component=daemon") != null);
    try std.testing.expect(std.mem.indexOf(u8, captured, "verdict=leak") != null);
}

test "check: .ok neither logs nor fails" {
    var bs = sinks.BufferedSink.init(std.testing.allocator);
    defer bs.deinit();
    sinks.clearSinksForTest();
    defer sinks.clearSinksForTest();
    sinks.registerSink(bs.sink());

    try leak_guard.check(.ok, "daemon");

    const captured = try bs.snapshot();
    defer std.testing.allocator.free(captured);
    try std.testing.expect(std.mem.indexOf(u8, captured, "gpa_leak_verdict") == null);
}
