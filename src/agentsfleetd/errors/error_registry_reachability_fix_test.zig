// A prior curation review had already confirmed these 18 pre-existing
// registry codes are dashboard-reachable but left them e()-only (no
// user_message). Turning on the reachable=>user_message standing guard
// forced curating them now rather than grandfathering the gap (Indy's call —
// see the spec's Discovery log). UZ-CONN-006, UZ-GRANT-002, UZ-GRANT-003 are
// covered in error_registry_promoted_test.zig; this file covers the
// remaining 15.
const std = @import("std");
const reg = @import("error_registry.zig");

pub fn expectCuratedDistinct(code: []const u8) !void {
    const entry = reg.lookup(code);
    const um = entry.user_message orelse return error.TestExpectedUserMessage;
    try std.testing.expect(um.len > 0);
    try std.testing.expect(!std.mem.eql(u8, um, entry.hint));
}

test "UZ-CONN-001/002/003/004 (connector platform) each have a curated user_message" {
    try expectCuratedDistinct(reg.ERR_CONNECTOR_NOT_CONFIGURED);
    try expectCuratedDistinct(reg.ERR_CONNECTOR_STATE_INVALID);
    try expectCuratedDistinct(reg.ERR_CONNECTOR_VENDOR_DEADLINE);
    try expectCuratedDistinct(reg.ERR_CONNECTOR_UNKNOWN);
}

test "UZ-AGT-008/010/011/012 (fleet lifecycle) each have a curated user_message" {
    try expectCuratedDistinct(reg.ERR_AGENTSFLEET_INVALID_CONFIG);
    try expectCuratedDistinct(reg.ERR_AGENTSFLEET_ALREADY_TERMINAL);
    try expectCuratedDistinct(reg.ERR_AGENTSFLEET_NAME_MISMATCH);
    try expectCuratedDistinct(reg.ERR_AGENTSFLEET_PAUSED_INGRESS);
}

test "UZ-BUNDLE-003/004/005 (fleet bundle install) each have a curated user_message" {
    try expectCuratedDistinct(reg.ERR_FLEET_BUNDLE_SECRETS_MISSING);
    try expectCuratedDistinct(reg.ERR_FLEET_BUNDLE_FETCH_FAILED);
    try expectCuratedDistinct(reg.ERR_FLEET_BUNDLE_STORAGE_UNAVAILABLE);
}

test "UZ-PROVIDER-005/006/007/008 (model library admin) each have a curated user_message" {
    try expectCuratedDistinct(reg.ERR_PROVIDER_BASE_URL_INVALID);
    try expectCuratedDistinct(reg.ERR_MODEL_CAP_NOT_FOUND);
    try expectCuratedDistinct(reg.ERR_MODEL_CAP_IN_USE);
    try expectCuratedDistinct(reg.ERR_MODEL_CAP_EXISTS);
}

test "UZ-RUN-014 (runner not found) has a curated user_message" {
    try expectCuratedDistinct(reg.ERR_RUNNER_NOT_FOUND);
}
