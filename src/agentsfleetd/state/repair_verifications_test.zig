const std = @import("std");
const repair_verifications = @import("repair_verifications.zig");

test "repair verifier actor selects completion metrics" {
    try std.testing.expect(repair_verifications.isVerifierEventActor(repair_verifications.VERIFIER_EVENT_ACTOR));
    try std.testing.expect(!repair_verifications.isVerifierEventActor("steer:operator"));
}
