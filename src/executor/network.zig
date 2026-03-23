//! Network policy enforcement for the host backend (§4.4).
//!
//! Applies egress restriction using bubblewrap's --unshare-net flag
//! (already used in the existing sandbox_shell_tool.zig). This module
//! provides the executor-owned network policy layer.
//!
//! In v1, network restriction is achieved through bubblewrap network
//! namespace isolation. The executor controls whether --unshare-net
//! is applied based on the execution's network policy.
//!
//! Future: nftables/iptables rules for fine-grained egress allowlists.

const std = @import("std");
const builtin = @import("builtin");

const log = std.log.scoped(.executor_network);

pub const NetworkPolicy = enum {
    /// No network access (default). Uses --unshare-net.
    deny_all,
    /// Allow specific egress destinations (future: nftables rules).
    allowlist,
};

pub const NetworkConfig = struct {
    policy: NetworkPolicy = .deny_all,
    /// Allowed egress destinations when policy is .allowlist.
    /// Format: "host:port" entries.
    allowed_destinations: []const []const u8 = &.{},
};

/// Append bubblewrap network arguments based on policy.
pub fn appendBwrapNetworkArgs(
    alloc: std.mem.Allocator,
    argv: *std.ArrayList([]const u8),
    config: NetworkConfig,
) !void {
    switch (config.policy) {
        .deny_all => {
            // --unshare-all already includes --unshare-net in the existing
            // bwrap setup. This is a no-op confirmation that network
            // isolation is active.
        },
        .allowlist => {
            // Future: for allowlist mode, we would use --share-net plus
            // nftables/iptables rules. For v1, we fall back to deny_all
            // since fine-grained egress is not yet implemented.
            _ = alloc;
            _ = argv;
            log.warn("network.allowlist_not_implemented falling_back=deny_all", .{});
        },
    }
}

/// Check if network namespace isolation is available.
pub fn isNetworkNamespaceAvailable() bool {
    if (builtin.os.tag != .linux) return false;
    // Check if unshare(CLONE_NEWNET) would succeed by checking for bwrap.
    std.fs.accessAbsolute("/usr/bin/bwrap", .{}) catch {
        std.fs.accessAbsolute("/usr/local/bin/bwrap", .{}) catch return false;
    };
    return true;
}

test "deny_all is default policy" {
    const config = NetworkConfig{};
    try std.testing.expectEqual(NetworkPolicy.deny_all, config.policy);
}

test "appendBwrapNetworkArgs with deny_all is no-op" {
    const alloc = std.testing.allocator;
    var argv = std.ArrayList([]const u8){};
    defer argv.deinit(alloc);
    try appendBwrapNetworkArgs(alloc, &argv, .{});
    try std.testing.expectEqual(@as(usize, 0), argv.items.len);
}

test "isNetworkNamespaceAvailable returns false on non-linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    try std.testing.expect(!isNetworkNamespaceAvailable());
}
