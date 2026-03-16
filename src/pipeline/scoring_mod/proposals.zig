const std = @import("std");
const pg = @import("pg");
const topology = @import("../topology.zig");
const id_format = @import("../../types/id_format.zig");

const ACTIVE_PROPOSAL_PLACEHOLDER = "[]";
const DISALLOWED_PROMPT_FIELD = "system_prompt_appendix";
const GENERATION_STATUS_PENDING = "PENDING";
const STATUS_PENDING_REVIEW = "PENDING_REVIEW";
const TRUST_LEVEL_TRUSTED = "TRUSTED";

const ProposalTriggerReason = enum {
    declining_score,
    sustained_low_score,

    fn label(self: ProposalTriggerReason) []const u8 {
        return switch (self) {
            .declining_score => "DECLINING_SCORE",
            .sustained_low_score => "SUSTAINED_LOW_SCORE",
        };
    }
};

const ApprovalMode = enum {
    auto,
    manual,

    fn label(self: ApprovalMode) []const u8 {
        return switch (self) {
            .auto => "AUTO",
            .manual => "MANUAL",
        };
    }
};

const RollingTrigger = struct {
    reason: ProposalTriggerReason,
};

const ActiveConfigContext = struct {
    trust_level: []u8,
    config_version_id: []u8,

    fn deinit(self: *ActiveConfigContext, alloc: std.mem.Allocator) void {
        alloc.free(self.trust_level);
        alloc.free(self.config_version_id);
    }
};

pub const ProposalValidationError = error{
    InvalidProposalJson,
    ProposalNotArray,
    ProposalChangeNotObject,
    MissingTargetField,
    DisallowedProposalField,
    UnregisteredAgentRef,
    InvalidSkillRef,
    EntitlementSkillNotAllowed,
};

pub fn maybePersistTriggerProposal(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    agent_id: []const u8,
    scored_at: i64,
) !void {
    const trigger = try detectRollingWindowTrigger(conn, agent_id) orelse return;
    var active_context = (try loadActiveConfigContext(conn, alloc, workspace_id, agent_id)) orelse return;
    defer active_context.deinit(alloc);

    try validateProposedChanges(conn, alloc, workspace_id, ACTIVE_PROPOSAL_PLACEHOLDER);

    const proposal_id = try id_format.generateTransitionId(alloc);
    defer alloc.free(proposal_id);

    const approval_mode = if (std.mem.eql(u8, active_context.trust_level, TRUST_LEVEL_TRUSTED))
        ApprovalMode.auto
    else
        ApprovalMode.manual;

    var q = try conn.query(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id,
        \\   approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NULL, $10, $11)
    , .{
        proposal_id,
        agent_id,
        workspace_id,
        trigger.reason.label(),
        ACTIVE_PROPOSAL_PLACEHOLDER,
        active_context.config_version_id,
        approval_mode.label(),
        GENERATION_STATUS_PENDING,
        STATUS_PENDING_REVIEW,
        scored_at,
        scored_at,
    });
    q.deinit();
}

pub fn validateProposedChanges(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    raw_json: []const u8,
) ProposalValidationError!void {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, raw_json, .{}) catch {
        return ProposalValidationError.InvalidProposalJson;
    };
    defer parsed.deinit();

    switch (parsed.value) {
        .array => |items| {
            for (items.items) |item| {
                try validateProposalChange(conn, workspace_id, item);
            }
        },
        else => return ProposalValidationError.ProposalNotArray,
    }
}

fn detectRollingWindowTrigger(conn: *pg.Conn, agent_id: []const u8) !?RollingTrigger {
    var q = try conn.query(
        \\SELECT score
        \\FROM agent_run_scores
        \\WHERE agent_id = $1
        \\ORDER BY scored_at DESC, score_id DESC
        \\LIMIT 10
    , .{agent_id});
    defer q.deinit();

    var scores: [10]i32 = undefined;
    var count: usize = 0;
    while (count < scores.len) {
        const row = (try q.next()) orelse break;
        scores[count] = try row.get(i32, 0);
        count += 1;
    }

    if (count < 5) return null;

    const current_sum = sumScores(scores[0..5]);
    if (current_sum < 300) {
        return .{ .reason = .sustained_low_score };
    }

    if (count < 10) return null;
    const previous_sum = sumScores(scores[5..10]);
    if (current_sum < previous_sum) {
        return .{ .reason = .declining_score };
    }

    return null;
}

fn sumScores(scores: []const i32) i32 {
    var total: i32 = 0;
    for (scores) |score| total += score;
    return total;
}

fn loadActiveConfigContext(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    agent_id: []const u8,
) !?ActiveConfigContext {
    var q = try conn.query(
        \\SELECT p.trust_level, active.config_version_id
        \\FROM agent_profiles p
        \\JOIN workspace_active_config active ON active.workspace_id = p.workspace_id
        \\WHERE p.workspace_id = $1 AND p.agent_id = $2
        \\LIMIT 1
    , .{ workspace_id, agent_id });
    defer q.deinit();

    const row = (try q.next()) orelse return null;
    return .{
        .trust_level = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .config_version_id = try alloc.dupe(u8, try row.get([]const u8, 1)),
    };
}

fn validateProposalChange(
    conn: *pg.Conn,
    workspace_id: []const u8,
    item: std.json.Value,
) ProposalValidationError!void {
    const obj = switch (item) {
        .object => |value| value,
        else => return ProposalValidationError.ProposalChangeNotObject,
    };

    const target_field_value = obj.get("target_field") orelse return ProposalValidationError.MissingTargetField;
    const target_field = switch (target_field_value) {
        .string => |value| value,
        else => return ProposalValidationError.MissingTargetField,
    };
    if (std.mem.eql(u8, target_field, DISALLOWED_PROMPT_FIELD)) {
        return ProposalValidationError.DisallowedProposalField;
    }

    const proposed_value = obj.get("proposed_value") orelse return;
    const proposed_obj = switch (proposed_value) {
        .object => |value| value,
        else => return,
    };

    if (stringField(proposed_obj, "agent_id")) |candidate_agent_id| {
        if (!agentExistsInWorkspace(conn, workspace_id, candidate_agent_id)) {
            return ProposalValidationError.UnregisteredAgentRef;
        }
    }

    const skill_ref = stringField(proposed_obj, "skill") orelse stringField(proposed_obj, "skill_id") orelse return;
    try validateSkillRef(conn, workspace_id, skill_ref);
}

fn stringField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const value = obj.get(key) orelse return null;
    return switch (value) {
        .string => |raw| raw,
        else => null,
    };
}

fn agentExistsInWorkspace(conn: *pg.Conn, workspace_id: []const u8, agent_id: []const u8) bool {
    var q = conn.query(
        \\SELECT 1
        \\FROM agent_profiles
        \\WHERE workspace_id = $1 AND agent_id = $2
        \\LIMIT 1
    , .{ workspace_id, agent_id }) catch return false;
    defer q.deinit();
    return (q.next() catch null) != null;
}

fn validateSkillRef(
    conn: *pg.Conn,
    workspace_id: []const u8,
    skill_ref: []const u8,
) ProposalValidationError!void {
    if (isCoreSkill(skill_ref)) return;
    if (!std.mem.startsWith(u8, skill_ref, "clawhub://") or !isPinnedSkillRef(skill_ref)) {
        return ProposalValidationError.InvalidSkillRef;
    }
    if (!workspaceAllowsCustomSkills(conn, workspace_id)) {
        return ProposalValidationError.EntitlementSkillNotAllowed;
    }
}

fn workspaceAllowsCustomSkills(conn: *pg.Conn, workspace_id: []const u8) bool {
    var q = conn.query(
        \\SELECT allow_custom_skills
        \\FROM workspace_entitlements
        \\WHERE workspace_id = $1
        \\LIMIT 1
    , .{workspace_id}) catch return false;
    defer q.deinit();
    const row = (q.next() catch null) orelse return false;
    return row.get(bool, 0) catch false;
}

fn isCoreSkill(skill_ref: []const u8) bool {
    return std.ascii.eqlIgnoreCase(skill_ref, topology.ROLE_ECHO) or
        std.ascii.eqlIgnoreCase(skill_ref, topology.ROLE_SCOUT) or
        std.ascii.eqlIgnoreCase(skill_ref, topology.ROLE_WARDEN);
}

fn isPinnedSkillRef(skill_ref: []const u8) bool {
    const at_idx = std.mem.lastIndexOfScalar(u8, skill_ref, '@') orelse return false;
    if (at_idx + 1 >= skill_ref.len) return false;
    return !std.ascii.eqlIgnoreCase(skill_ref[at_idx + 1 ..], "latest");
}
