//! Observer runtime for NullClaw — selects backend from env.
//!
//! Reads `NULLCLAW_OBSERVER` (case-insensitive: `noop` | `verbose` | else
//! defaults to log backend). Owns one inline instance of each backend so
//! callers can flip between them without re-allocating.

const std = @import("std");
const nullclaw = @import("nullclaw");
const observability = nullclaw.observability;

const ObserverRuntime = @This();

backend: ObserverBackend,
noop: observability.NoopObserver = .{},
log_observer: observability.LogObserver = .{},
verbose_observer: observability.VerboseObserver = .{},

pub const ObserverBackend = enum { log_backend, noop, verbose };

pub fn init(alloc: std.mem.Allocator) ObserverRuntime {
    const raw = std.process.getEnvVarOwned(alloc, "NULLCLAW_OBSERVER") catch return .{ .backend = .log_backend };
    defer alloc.free(raw);
    return .{ .backend = parseBackend(raw) };
}

/// Pure mapping for `NULLCLAW_OBSERVER` values. Exposed for tests that
/// need to exercise the case-insensitive match without forking the
/// process or mutating environment variables.
pub fn parseBackend(raw: []const u8) ObserverBackend {
    if (std.ascii.eqlIgnoreCase(raw, "noop")) return .noop;
    if (std.ascii.eqlIgnoreCase(raw, "verbose")) return .verbose;
    return .log_backend;
}

pub fn observer(self: *ObserverRuntime) observability.Observer {
    return switch (self.backend) {
        .log_backend => self.log_observer.observer(),
        .noop => self.noop.observer(),
        .verbose => self.verbose_observer.observer(),
    };
}

test {
    _ = @import("runner_observer_test.zig");
}
