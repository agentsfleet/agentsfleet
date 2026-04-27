//! Shared key-name convention for workspace zombie credentials.
//!
//! The HTTP handler stores rows under "zombie:<name>"; the worker resolver
//! loads them under the same prefix. Owning the constant in one place stops
//! the two callers from drifting silently — a divergence would make every
//! worker-side lookup miss its row with `error.NotFound`.
//!
//! Sits outside `vault.zig` on purpose: vault is naming-agnostic by design,
//! and BYOK rows (key_name = "llm") use the same vault layer without this
//! prefix.

const std = @import("std");

const PREFIX = "zombie:";

/// Compose the storage key for a zombie credential. Caller owns the slice
/// and must free it with the same allocator.
pub fn allocKeyName(alloc: std.mem.Allocator, name: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, PREFIX ++ "{s}", .{name});
}
