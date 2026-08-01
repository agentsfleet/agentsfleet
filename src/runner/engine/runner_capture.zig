//! Progress-fd wiring for a run: which observer the fleet reports through, and
//! whether in-run memory deltas are captured.
//!
//! Both answers turn on the same fact — whether the lease gave us a progress fd
//! to stream on. Split out of `runner.zig` (RULE FLL) so that file stays under
//! the length cap; the caller owns the `writer`/`adapter` storage because the
//! returned observer's vtable captures `&adapter` and must outlive this call.

const std = @import("std");
const nullclaw = @import("nullclaw");

const memory_mod = nullclaw.memory;
const observability = nullclaw.observability;

const inrun_memory = @import("inrun_memory.zig");
const runner_progress = @import("runner_progress.zig");

/// Build the in-run memory capturer iff there is both a progress fd to write the
/// `.memory` frame on and a live store to read. Null otherwise — capture is a
/// no-op on the non-streaming/test path and when the store failed to build.
pub fn makeCapturer(
    progress_fd: ?std.posix.fd_t,
    mem_opt: ?memory_mod.Memory,
    alloc: std.mem.Allocator,
) ?inrun_memory.MemoryCapturer {
    const fd = progress_fd orelse return null;
    const mem = mem_opt orelse return null;
    return .{ .mem = mem, .fd = fd, .alloc = alloc };
}

/// Pick the fleet's observer. With a progress fd, init the caller-owned
/// `writer`/`adapter` in place (so the returned observer's vtable, which
/// captures `&adapter`, stays valid for the run) and return the redacting
/// Adapter's observer; otherwise the env-selected backend.
pub fn selectObserver(
    progress_fd: ?std.posix.fd_t,
    fallback: observability.Observer,
    writer: *runner_progress.ProgressWriter,
    adapter: *runner_progress.Adapter,
    alloc: std.mem.Allocator,
    secrets: []const runner_progress.Secret,
) observability.Observer {
    const fd = progress_fd orelse return fallback;
    writer.* = .{ .fd = fd, .alloc = alloc };
    adapter.* = .{ .writer = writer, .alloc = alloc, .secrets = secrets };
    return adapter.observer();
}

test "no progress fd means no capturer and the fallback observer" {
    // The non-streaming path: nothing to write frames on, so capture is off and
    // the env-selected backend stays in place.
    try std.testing.expect(makeCapturer(null, null, std.testing.allocator) == null);
}
