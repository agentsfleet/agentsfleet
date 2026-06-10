//! network.zig — facade for the per-lease egress stack (the `std.net` shape):
//! callers import this, never the leaf files. `AllowList` (the merged L4+L7
//! egress allowlist), `Plan` (pure per-lease derivation), the netlink
//! serializers, the Linux-only `Socket`, and the `EgressScope` lifecycle.

pub const AllowList = @import("AllowList.zig");
pub const Policy = @import("Policy.zig");
pub const Plan = @import("Plan.zig");
pub const EgressScope = @import("EgressScope.zig");
pub const Socket = @import("Socket.zig");
pub const MessageBuilder = @import("MessageBuilder.zig");
pub const rtnetlink = @import("rtnetlink.zig");
pub const nfnetlink = @import("nfnetlink.zig");
pub const nfnetlink_rule = @import("nfnetlink_rule.zig");

test {
    _ = AllowList;
    _ = Policy;
    _ = Plan;
    _ = EgressScope;
    _ = Socket;
    _ = MessageBuilder;
    _ = rtnetlink;
    _ = nfnetlink;
    _ = nfnetlink_rule;
    _ = @import("nfnetlink_rule_test.zig");
}
