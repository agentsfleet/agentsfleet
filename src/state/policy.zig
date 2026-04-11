//! Policy enforcement — stub after M10_001 pipeline v1 removal.
//!
//! The policy_events table and runs table were dropped. recordPolicyEvent is
//! now a no-op that logs the decision without persisting to DB. Callers
//! (workspaces_ops.zig) are unchanged — they catch the error return.

const std = @import("std");
const pg = @import("pg");
const types = @import("../types.zig");
const events = @import("../events/bus.zig");
const log = std.log.scoped(.policy);

/// Record a policy decision. M10_001: policy_events table dropped — log only.
pub fn recordPolicyEvent(
    conn: *pg.Conn,
    workspace_id: []const u8,
    run_id: ?[]const u8,
    action_class: types.ActionClass,
    decision: types.PolicyDecision,
    rule_id: []const u8,
    actor: []const u8,
) !void {
    _ = conn;
    log.info("policy.event workspace={s} class={s} decision={s} rule={s} actor={s}", .{
        workspace_id,
        @tagName(action_class),
        @tagName(decision),
        rule_id,
        actor,
    });
    events.emit("policy_event", run_id, "policy_event");
}
