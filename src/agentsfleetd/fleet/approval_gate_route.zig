//! The approval gate's ORDERING decision, isolated from its I/O.
//!
//! `checkApprovalGate` reads two things — whether this event already has a
//! recorded gate, and what the fleet's current policy says — and the ORDER in
//! which those two bind is a security property rather than a style choice. This
//! module is that order, expressed as a pure function over two small enums, so it
//! can be pinned by unit tests instead of by a live Redis and Postgres.
//!
//! Split from `approval_gate.zig` for the file-length budget (RULE FLL).

const std = @import("std");
const approval_gate = @import("../fleet_runtime/approval_gate.zig");

/// The recorded-gate lookup outcome, without its payload. `unreadable` stays
/// distinct from `absent` because collapsing them is unsafe in both directions:
/// absent means this event was never parked, unreadable means we cannot tell —
/// and raising a SECOND approval card for an event that may already hold one is
/// worse than waiting a poll.
pub const RefState = enum { found, absent, unreadable };

/// What the caller does once the lookup and the policy have both spoken.
pub const Route = enum {
    /// The recorded gate decides; policy is not consulted at all.
    evaluate_recorded,
    pass,
    kill,
    /// Policy wants a gate and none is recorded — raise one.
    request_new,
    /// Policy wants a gate but we could not read whether one already exists.
    wait,
};

/// Joint meaning of a recorded-gate lookup and a policy decision. `decision` is
/// null when the fleet declares no gate policy at all.
///
/// `.found` outranks EVERY policy outcome — including both that would otherwise
/// pass: no `gates` at all, and `.auto_approve` from an emptied `rules`. Those
/// two are exactly what a mid-flight `config_json` PATCH produces, and honouring
/// the recorded gate ahead of them is what stops such a PATCH from silently
/// withdrawing a question already put to a human. Waking a fleet and
/// reconfiguring one are ONE scope today (`fleet:write`), so that PATCH asks for
/// no approval of its own; splitting `fleet:message` out of it is its own piece
/// of work, but a gate this daemon already raised does not have to wait for that.
pub fn route(state: RefState, decision: ?approval_gate.GateDecision) Route {
    if (state == .found) return .evaluate_recorded;
    const d = decision orelse return .pass;
    return switch (d) {
        .auto_approve => .pass,
        .auto_kill => .kill,
        .requires_approval => switch (state) {
            .found => unreachable, // returned above
            .unreadable => .wait,
            .absent => .request_new,
        },
    };
}

// ── Tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

test "route: a recorded gate outranks every policy outcome" {
    // The two rows that carry the security property. Both are what a mid-flight
    // `config_json` PATCH produces, and under the previous ordering both released
    // the parked event while its approval card still sat unanswered in Slack.
    try testing.expectEqual(Route.evaluate_recorded, route(.found, null)); // `gates` dropped
    try testing.expectEqual(Route.evaluate_recorded, route(.found, .auto_approve)); // `rules` emptied

    // The remaining outcomes, so "outranks EVERY policy outcome" is asserted
    // rather than merely claimed by the two rows above.
    try testing.expectEqual(Route.evaluate_recorded, route(.found, .requires_approval));
    try testing.expectEqual(Route.evaluate_recorded, route(.found, .auto_kill));
}

test "route: with no recorded gate, policy decides exactly as before" {
    try testing.expectEqual(Route.pass, route(.absent, null));
    try testing.expectEqual(Route.pass, route(.absent, .auto_approve));
    try testing.expectEqual(Route.kill, route(.absent, .auto_kill));
    try testing.expectEqual(Route.request_new, route(.absent, .requires_approval));
}

test "route: an unreadable lookup waits rather than raising a second card" {
    // A Redis blip must not re-notify a human who may already hold a card for
    // this exact event — but it must also not stall a fleet that wants no gate,
    // which is why only the gated row waits.
    try testing.expectEqual(Route.wait, route(.unreadable, .requires_approval));
    try testing.expectEqual(Route.pass, route(.unreadable, null));
    try testing.expectEqual(Route.pass, route(.unreadable, .auto_approve));
    try testing.expectEqual(Route.kill, route(.unreadable, .auto_kill));
}
