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

const ObserverBackend = enum { log_backend, noop, verbose };

pub fn init(alloc: std.mem.Allocator) ObserverRuntime {
    const raw = std.process.getEnvVarOwned(alloc, "NULLCLAW_OBSERVER") catch return .{ .backend = .log_backend };
    defer alloc.free(raw);
    const backend: ObserverBackend = if (std.ascii.eqlIgnoreCase(raw, "noop"))
        .noop
    else if (std.ascii.eqlIgnoreCase(raw, "verbose"))
        .verbose
    else
        .log_backend;
    return .{ .backend = backend };
}

pub fn observer(self: *ObserverRuntime) observability.Observer {
    return switch (self.backend) {
        .log_backend => self.log_observer.observer(),
        .noop => self.noop.observer(),
        .verbose => self.verbose_observer.observer(),
    };
}
