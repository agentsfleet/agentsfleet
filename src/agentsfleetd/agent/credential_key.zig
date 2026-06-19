//! Shared key-name convention for workspace agent credentials.
//!
//! The HTTP handler stores rows under "agent:<name>"; worker and provider
//! resolvers load dashboard-created rows under the same prefix. Owning the
//! constant in one place stops callers from drifting silently — a divergence
//! would make lookups miss their row with `error.NotFound`.
//!
//! Sits outside `vault.zig` on purpose: vault is naming-agnostic by design,
//! and self-managed provider records (user-named) use the same vault layer without
//! this prefix.

const std = @import("std");

const PREFIX = "agent:";

/// Compose the storage key for an agent credential. Caller owns the slice
/// and must free it with the same allocator.
pub fn allocKeyName(alloc: std.mem.Allocator, name: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, PREFIX ++ "{s}", .{name});
}
