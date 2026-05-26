const std = @import("std");
const testing = std.testing;
const contact = @import("contact.zig");

// Pin test — bumping SUPPORT_EMAIL must be a coordinated change across
// every runtime (Zig + website TS + app TS + zombiectl JS + Mintlify
// snippet). The literal IS the contract here; this test fails until
// the matching pin tests in each sibling runtime catch up.
test "SUPPORT_EMAIL pinned to usezombie@agentmail.to" {
    // pin test: literal is the contract
    try testing.expectEqualStrings("usezombie@agentmail.to", contact.SUPPORT_EMAIL);
}
