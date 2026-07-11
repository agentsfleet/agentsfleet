//! Turns a `DebugAllocator.deinit()` verdict into a hard failure at teardown.
//!
//! The daemon and the runner worker pool each own a `DebugAllocator` whose
//! `deinit()` returns a leak verdict that was historically discarded (`_ = …`).
//! Per A1 (`dispatch/write_zig.md`) the debug GPA is the backing that detects
//! leaks; a release build picks a fast allocator and owns no GPA verdict. A
//! `.leak` is therefore a debug/test-time signal — the guard fails on it in
//! Debug builds and only *logs* it elsewhere, so ReleaseSafe production keeps
//! its current best-effort shutdown (no abort) and operational behaviour is
//! unchanged.
//!
//! Lives in the `log` module (not `common`): its one dependency is structured
//! logging, and `log` already imports `common`, so a guard in `common` would
//! close a `common → log → common` import cycle.

const std = @import("std");
const builtin = @import("builtin");
const logging = @import("mod.zig");

const log = logging.scoped(.leak_guard);

/// Returned by `verdictToError`/`check` on a `.leak` verdict in Debug builds so
/// the caller's teardown (daemon exit, pool join) fails instead of discarding it.
pub const LeakError = error{LeakDetected};

/// Pure verdict → result: `.leak` fails in Debug builds only, `.ok` always
/// passes. No logging — so the fail-on-warn test runner can assert it directly
/// (a `.leak` path through `check` would emit an `err` log the harness counts as
/// a test failure). The real teardown sites call `check`, which logs first.
pub fn verdictToError(verdict: std.heap.Check) LeakError!void {
    if (verdict == .leak and comptime builtin.mode == .Debug) return error.LeakDetected;
}

/// Log-then-assert wrapper for the real teardown sites. On `.leak` it logs
/// `gpa_leak_verdict` (component + verdict) in *every* build so operators see it
/// in ReleaseSafe too, then applies `verdictToError` (fails in Debug only).
pub fn check(verdict: std.heap.Check, component: []const u8) LeakError!void {
    if (verdict == .leak) log.err("gpa_leak_verdict", .{ .component = component, .verdict = "leak" });
    return verdictToError(verdict);
}
