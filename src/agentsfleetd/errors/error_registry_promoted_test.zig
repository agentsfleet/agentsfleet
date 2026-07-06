// Jargon-leaking internalOperationError() sites promoted off the shared
// UZ-INTERNAL-003 bucket into their own registry entries. Each gets a
// negative test asserting the code resolves and (for the eu()-curated ones)
// carries a distinct, non-jargon user_message. Split from error_registry_test.zig
// to keep that file under the 350-line cap.

const std = @import("std");
const reg = @import("error_registry.zig");
const reachability_fix = @import("error_registry_reachability_fix_test.zig");

const expectCuratedDistinct = reachability_fix.expectCuratedDistinct;

test "UZ-AUTH-023 (Clerk webhook secret not configured) is registered" {
    const entry = reg.lookup(reg.ERR_CLERK_WEBHOOK_SECRET_NOT_CONFIGURED);
    try std.testing.expect(entry.hint.len > 0);
    try std.testing.expect(entry.user_message == null); // operator-only path, not dashboard-reachable
}

test "UZ-CONN-007 (connector catalog lookup failed) has a curated user_message" {
    try expectCuratedDistinct(reg.ERR_CONNECTOR_CATALOG_LOOKUP_FAILED);
    const um = reg.lookup(reg.ERR_CONNECTOR_CATALOG_LOOKUP_FAILED).user_message.?;
    try std.testing.expect(std.mem.indexOf(u8, um, "vault") == null);
}

test "UZ-AGT-013 (fleet install rolled back) has a curated user_message" {
    try expectCuratedDistinct(reg.ERR_AGENTSFLEET_INSTALL_ROLLED_BACK);
    const um = reg.lookup(reg.ERR_AGENTSFLEET_INSTALL_ROLLED_BACK).user_message.?;
    try std.testing.expect(std.mem.indexOf(u8, um, "event-stream") == null);
}

test "UZ-CRED-002 (credential broker not configured) is registered" {
    const entry = reg.lookup(reg.ERR_CRED_BROKER_NOT_CONFIGURED);
    try std.testing.expect(entry.hint.len > 0);
    try std.testing.expect(entry.user_message == null); // runner-only path, not dashboard-reachable
}

test "UZ-PROVIDER-010 (tenant has no primary workspace) has a curated user_message" {
    try expectCuratedDistinct(reg.ERR_TENANT_NO_PRIMARY_WORKSPACE);
    const um = reg.lookup(reg.ERR_TENANT_NO_PRIMARY_WORKSPACE).user_message.?;
    try std.testing.expect(std.mem.indexOf(u8, um, "invariant") == null);
}

// A prior curation review had already confirmed these two are
// dashboard-reachable but left them e()-only — missed on the first
// promotion pass, caught by the standing reachable=>user_message guard.
test "UZ-CONN-006 (connector OAuth exchange failed) has a curated user_message" {
    try expectCuratedDistinct(reg.ERR_CONNECTOR_OAUTH_EXCHANGE_FAILED);
}

test "UZ-GRANT-003 (grant already resolved) has a curated user_message" {
    try expectCuratedDistinct(reg.ERR_GRANT_ALREADY_RESOLVED);
}
