//! Generation-guarded socket ownership — the stable control block a scheduled
//! deadline points at.
//!
//! One of these lives inside each exclusive network owner (an HTTP client
//! wrapper, a Redis attempt). It exists BEFORE connection setup starts, so a
//! deadline can bound name resolution and dialling, which have no socket yet.
//! `beginAttempt` advances the generation before the attempt can be
//! interrupted, which is what makes a late fire against a replaced connection
//! provably a no-op rather than a cross-connection kill.
//!
//! The owner drives it as: `beginAttempt` → (arm a scheduler guard on `target`)
//! → `attachSocket` once a socket exists → check `wasInterrupted` between setup
//! stages → `endAttempt` on completion → `guard.finish()`. Finishing the guard
//! is the quiescence barrier: after it returns, no interrupt callback is
//! running or can start, so the owner may move or free itself.

const SocketOwner = @This();

/// Guards generation, handle, and the interrupted flag together. Held across
/// the shutdown syscall so a completing attempt cannot swap in a recycled
/// descriptor between the generation check and the call.
mutex: common.Mutex = .{},
/// Monotonic and non-repeating. Generation 0 means "no attempt has begun", so
/// it never matches a live registration.
generation: u64 = 0,
/// The socket for the CURRENT generation, once one exists.
handle: ?std.posix.fd_t = null,
/// Set when a deadline fired against the current generation. Cleared by the
/// next `beginAttempt`; read by the owner between setup stages.
interrupted: bool = false,

/// Advance to a fresh generation before setup begins. Any registration still
/// naming the previous generation is now stale.
pub fn beginAttempt(self: *SocketOwner) u64 {
    self.mutex.lock();
    defer self.mutex.unlock();
    self.generation += 1;
    self.handle = null;
    self.interrupted = false;
    return self.generation;
}

/// Publish the socket for `generation`. A mismatch means the attempt was
/// already retired, so the caller owns closing `handle` itself.
pub fn attachSocket(self: *SocketOwner, generation: u64, handle: std.posix.fd_t) bool {
    self.mutex.lock();
    defer self.mutex.unlock();
    if (self.generation != generation) return false;
    self.handle = handle;
    return true;
}

/// Retire the current generation. A deadline that fires after this is stale,
/// so the owner may release its socket back to a pool.
pub fn endAttempt(self: *SocketOwner) void {
    self.mutex.lock();
    defer self.mutex.unlock();
    self.generation += 1;
    self.handle = null;
}

/// True when a deadline fired against the current attempt. Checked between
/// setup stages so one budget bounds resolve → dial → handshake → subscribe.
pub fn wasInterrupted(self: *SocketOwner) bool {
    self.mutex.lock();
    defer self.mutex.unlock();
    return self.interrupted;
}

/// Build the handle to arm a scheduler guard with. Valid only for `generation`.
pub fn target(self: *SocketOwner, generation: u64) InterruptTarget {
    return .{ .ctx = self, .interruptFn = interruptThunk, .generation = generation };
}

/// Runs on the scheduler worker. Bounded and nonblocking: it takes the owner
/// lock and issues at most one `shutdown(2)`, which does not block.
fn interruptThunk(ctx: *anyopaque, expected_generation: u64) InterruptTarget.Outcome {
    const self: *SocketOwner = @ptrCast(@alignCast(ctx));
    self.mutex.lock();
    defer self.mutex.unlock();
    if (self.generation != expected_generation) return .stale;
    self.interrupted = true;
    // Shut down UNDER the lock. Releasing it first would let the owner complete
    // and a successor attach a recycled descriptor number between the check and
    // the syscall — exactly the cross-connection kill this type prevents.
    if (self.handle) |handle| shutdownSocket(handle);
    return .interrupted;
}

/// Best-effort `shutdown(2)`, no libc required. Linux issues the syscall
/// directly so this compiles in the `test-lib` graph, which does not link libc
/// and where `std.c` is therefore a compile error. macOS has no stable syscall
/// ABI and always links libc, so it keeps the `std.c` path.
fn shutdownSocket(handle: std.posix.fd_t) void {
    if (comptime builtin.os.tag == .linux) {
        _ = std.os.linux.shutdown(handle, std.os.linux.SHUT.RDWR);
    } else {
        _ = std.c.shutdown(handle, std.c.SHUT.RDWR);
    }
}

test {
    _ = @import("SocketOwner_test.zig");
}

const std = @import("std");
const builtin = @import("builtin");
const common = @import("common");
const InterruptTarget = @import("InterruptTarget.zig");
