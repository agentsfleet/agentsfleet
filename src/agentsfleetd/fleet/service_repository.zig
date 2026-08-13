//! The ONE conversion between the repository binding a fleet AUTHORED and the
//! binding a lease is TOLD.
//!
//! Split from `service.zig` for the file-length budget (RULE FLL), the same way
//! `service_endpoint.zig` was.

const std = @import("std");
const integration = @import("../credentials/integration.zig");
const wire = @import("contract");
const execution_policy = wire.execution_policy;

/// The repository slices are borrowed from the session's config, which outlives
/// the response serialization (its deinit is deferred in `issueLease`) — the
/// same borrow `provider`/`model` already rely on.
///
/// Absent binding stays absent: the runner then refuses every fetch, exactly as
/// the mint already refuses to issue a token. The two rings fail closed together.
pub fn wireRepositoryBinding(authored: ?integration.RepositoryBinding) ?execution_policy.RepositoryBinding {
    const binding = authored orelse return null;
    return .{
        .repositories = binding.repositories,
        .access = switch (binding.access) {
            .read => .read,
            .write => .write,
        },
        .base_branch = binding.base_branch,
    };
}

test "the authored repository binding reaches the lease unchanged, and an absent one stays absent" {
    // The conversion's `switch` is exhaustive, so a new authored access level is
    // a compile error here rather than a silently dropped one. This pins the
    // other half: that the two enums SPELL the same values, which is what makes
    // the runner's refusal and the mint's scoping agree about one binding.
    const repos = [_][]const u8{ "acme/payments", "acme/ledger" };
    const carried = wireRepositoryBinding(.{ .repositories = &repos, .access = .write, .base_branch = "main" }).?;
    try std.testing.expectEqual(@as(usize, 2), carried.repositories.len);
    try std.testing.expectEqualStrings("acme/payments", carried.repositories[0]);
    try std.testing.expectEqual(execution_policy.RepositoryAccess.write, carried.access);
    try std.testing.expectEqualStrings("main", carried.base_branch.?);
    try std.testing.expectEqualStrings(
        @tagName(integration.RepositoryAccess.read),
        @tagName(execution_policy.RepositoryAccess.read),
    );

    // Fail closed: no binding on the fleet means no binding on the lease, so the
    // runner refuses every fetch exactly as the mint refuses every token.
    try std.testing.expect(wireRepositoryBinding(null) == null);
}
