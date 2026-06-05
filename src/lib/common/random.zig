//! Project-facing CSPRNG. Zig 0.16 removed `std.crypto.random` and
//! `std.posix.getrandom`; OS entropy now lives behind the `Io` interface.
//! `secureRandomBytes` is the ONE entropy API every credential/token/key path
//! calls — a single audit + policy surface, never `globalIo().randomSecure()`
//! scattered at call sites.

const std = @import("std");
const sync = @import("sync.zig");

/// Fill `buf` with cryptographically-secure OS entropy. Backed by
/// `io.randomSecure` (always syscalls, no stored RNG state → fork-safe). The
/// fork-unsafe seeded `io.random` is intentionally never exposed.
pub fn secureRandomBytes(buf: []u8) std.Io.RandomSecureError!void {
    return sync.globalIo().randomSecure(buf);
}
