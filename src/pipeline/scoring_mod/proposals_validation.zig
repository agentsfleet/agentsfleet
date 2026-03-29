const std = @import("std");
const pg = @import("pg");
const topology = @import("../topology.zig");
const entitlements = @import("../../state/entitlements.zig");
const shared = @import("proposals_shared.zig");

const log = std.log.scoped(.scoring);

pub const ProposalValidationError = shared.ProposalError;

pub fn validateProposedChanges(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    config_version_id: []const u8,
    raw_json: []const u8,
) (ProposalValidationError || anyerror)!void {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, raw_json, .{}) catch {
        return ProposalValidationError.InvalidProposalJson;
    };
    defer parsed.deinit();

    log.debug("validating proposed changes workspace_id={s} config_version_id={s}", .{ workspace_id, config_version_id });

    switch (parsed.value) {
        .array => |items| {
            log.debug("validating {d} proposal changes", .{items.items.len});
            for (items.items) |item| {
                try validateProposalChange(conn, workspace_id, item);
            }
            const candidate_profile_json = try applyProposalChangesToConfig(conn, alloc, config_version_id, items.items);
            defer alloc.free(candidate_profile_json);

            var candidate_profile = topology.parseProfileJson(alloc, candidate_profile_json) catch {
                log.warn("proposal would not compile workspace_id={s}", .{workspace_id});
                return ProposalValidationError.ProposalWouldNotCompile;
            };
            defer candidate_profile.deinit();
            try validateCandidateProfileSkills(conn, workspace_id, &candidate_profile);

            entitlements.enforceWithAudit(
                conn,
                alloc,
                workspace_id,
                config_version_id,
                candidate_profile_json,
                .compile,
                shared.PROPOSAL_ACTOR,
            ) catch |err| switch (err) {
                entitlements.EnforcementError.EntitlementProfileLimit => return ProposalValidationError.EntitlementProfileLimit,
                entitlements.EnforcementError.EntitlementStageLimit => return ProposalValidationError.EntitlementStageLimit,
                entitlements.EnforcementError.EntitlementSkillNotAllowed => return ProposalValidationError.EntitlementSkillNotAllowed,
                entitlements.EnforcementError.InvalidCompiledProfile, entitlements.EnforcementError.EntitlementMissing => return ProposalValidationError.ProposalWouldNotCompile,
                else => return ProposalValidationError.ProposalWouldNotCompile,
            };
        },
        else => {
            log.warn("proposal body is not an array", .{});
            return ProposalValidationError.ProposalNotArray;
        },
    }
}

pub fn buildCandidateProfileJson(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    config_version_id: []const u8,
    raw_json: []const u8,
) (ProposalValidationError || anyerror)![]u8 {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, raw_json, .{}) catch {
        return ProposalValidationError.InvalidProposalJson;
    };
    defer parsed.deinit();

    return switch (parsed.value) {
        .array => |items| applyProposalChangesToConfig(conn, alloc, config_version_id, items.items),
        else => ProposalValidationError.ProposalNotArray,
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

    const target_field_value = obj.get(shared.JSON_KEY_TARGET_FIELD) orelse return ProposalValidationError.MissingTargetField;
    const target_field = switch (target_field_value) {
        .string => |value| value,
        else => return ProposalValidationError.MissingTargetField,
    };
    if (std.mem.eql(u8, target_field, shared.DISALLOWED_PROMPT_FIELD)) {
        log.warn("rejected disallowed proposal field workspace_id={s}", .{workspace_id});
        return ProposalValidationError.DisallowedProposalField;
    }
    if (!std.mem.eql(u8, target_field, shared.PROPOSAL_TARGET_STAGE_BINDING) and
        !std.mem.eql(u8, target_field, shared.PROPOSAL_TARGET_STAGE_INSERT))
    {
        log.warn("rejected unsupported target_field={s}", .{target_field});
        return ProposalValidationError.UnsupportedTargetField;
    }

    const proposed_value = obj.get(shared.JSON_KEY_PROPOSED_VALUE) orelse return ProposalValidationError.InvalidProposalJson;
    const proposed_obj = switch (proposed_value) {
        .object => |value| value,
        else => return ProposalValidationError.InvalidProposalJson,
    };

    if (stringField(proposed_obj, shared.JSON_KEY_AGENT_ID)) |candidate_agent_id| {
        if (!agentExistsInWorkspace(conn, workspace_id, candidate_agent_id)) {
            log.warn("rejected unregistered agent_id ref workspace_id={s}", .{workspace_id});
            return ProposalValidationError.UnregisteredAgentRef;
        }
    }

    const stage_id = stringField(proposed_obj, shared.JSON_KEY_STAGE_ID) orelse stringField(obj, shared.JSON_KEY_STAGE_ID) orelse return ProposalValidationError.MissingStageId;
    if (stage_id.len == 0) return ProposalValidationError.MissingStageId;
    const role_id = stringField(proposed_obj, shared.JSON_KEY_ROLE) orelse return ProposalValidationError.MissingRole;
    if (role_id.len == 0) return ProposalValidationError.MissingRole;
    if (std.mem.eql(u8, target_field, shared.PROPOSAL_TARGET_STAGE_INSERT)) {
        const insert_before_stage_id = stringField(proposed_obj, shared.JSON_KEY_INSERT_BEFORE_STAGE_ID) orelse return ProposalValidationError.MissingInsertBeforeStageId;
        if (insert_before_stage_id.len == 0) return ProposalValidationError.MissingInsertBeforeStageId;
    }

    const skill_ref = stringField(proposed_obj, shared.JSON_KEY_SKILL) orelse stringField(proposed_obj, shared.JSON_KEY_SKILL_ID) orelse return ProposalValidationError.InvalidSkillRef;
    try validateSkillRef(conn, workspace_id, skill_ref);
}

pub fn applyProposalChangesToConfig(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    config_version_id: []const u8,
    changes: []const std.json.Value,
) (ProposalValidationError || anyerror)![]u8 {
    var profile = try loadConfigProfile(conn, alloc, config_version_id);
    defer profile.deinit();

    var stages: std.ArrayList(topology.Stage) = .{};
    defer {
        for (stages.items) |stage| freeStage(alloc, stage);
        stages.deinit(alloc);
    }
    for (profile.stages) |stage| try stages.append(alloc, try cloneStage(alloc, stage));

    for (changes) |change| {
        try applySingleChange(alloc, &stages, change);
    }

    // Deep-clone gate_tools to avoid double-free (each GateTool owns name/command slices).
    var gt_cloned: std.ArrayList(topology.GateTool) = .{};
    errdefer {
        for (gt_cloned.items) |gt| {
            alloc.free(gt.name);
            alloc.free(gt.command);
        }
        gt_cloned.deinit(alloc);
    }
    for (profile.gate_tools) |gt| {
        try gt_cloned.append(alloc, .{
            .name = try alloc.dupe(u8, gt.name),
            .command = try alloc.dupe(u8, gt.command),
            .timeout_ms = gt.timeout_ms,
        });
    }
    var candidate = topology.Profile{
        .agent_id = try alloc.dupe(u8, profile.agent_id),
        .stages = try stages.toOwnedSlice(alloc),
        .gate_tools = try gt_cloned.toOwnedSlice(alloc),
        .max_repair_loops = profile.max_repair_loops,
        .alloc = alloc,
    };
    defer candidate.deinit();
    return stringifyProfileJson(alloc, &candidate);
}

fn applySingleChange(
    alloc: std.mem.Allocator,
    stages: *std.ArrayList(topology.Stage),
    change: std.json.Value,
) (ProposalValidationError || anyerror)!void {
    const obj = switch (change) {
        .object => |value| value,
        else => return ProposalValidationError.ProposalChangeNotObject,
    };
    const target_field = switch (obj.get(shared.JSON_KEY_TARGET_FIELD) orelse return ProposalValidationError.MissingTargetField) {
        .string => |value| value,
        else => return ProposalValidationError.MissingTargetField,
    };
    const proposed_obj = switch (obj.get(shared.JSON_KEY_PROPOSED_VALUE) orelse return ProposalValidationError.InvalidProposalJson) {
        .object => |value| value,
        else => return ProposalValidationError.InvalidProposalJson,
    };

    if (std.mem.eql(u8, target_field, shared.PROPOSAL_TARGET_STAGE_INSERT)) {
        const insert_before_stage_id = stringField(proposed_obj, shared.JSON_KEY_INSERT_BEFORE_STAGE_ID) orelse return ProposalValidationError.MissingInsertBeforeStageId;
        const insert_index = indexOfStage(stages.items, insert_before_stage_id) orelse return ProposalValidationError.UnknownStageRef;
        const new_stage = try stageFromProposalValue(alloc, proposed_obj);
        errdefer freeStage(alloc, new_stage);
        if (indexOfStage(stages.items, new_stage.stage_id) != null) return ProposalValidationError.DuplicateStageRef;
        try stages.insert(alloc, insert_index, new_stage);
        return;
    }

    if (std.mem.eql(u8, target_field, shared.PROPOSAL_TARGET_STAGE_BINDING)) {
        const stage_id = stringField(proposed_obj, shared.JSON_KEY_STAGE_ID) orelse stringField(obj, shared.JSON_KEY_STAGE_ID) orelse return ProposalValidationError.MissingStageId;
        const stage_index = indexOfStage(stages.items, stage_id) orelse return ProposalValidationError.UnknownStageRef;
        const replacement = try stageFromProposalValue(alloc, proposed_obj);
        freeStage(alloc, stages.items[stage_index]);
        stages.items[stage_index] = replacement;
        return;
    }

    return ProposalValidationError.UnsupportedTargetField;
}

fn loadConfigProfile(conn: *pg.Conn, alloc: std.mem.Allocator, config_version_id: []const u8) !topology.Profile {
    var q = try conn.query(
        \\SELECT compiled_profile_json
        \\FROM agent_config_versions
        \\WHERE config_version_id = $1
        \\LIMIT 1
    , .{config_version_id});
    const row = (try q.next()) orelse {
        // null return → 'C'+'Z' already consumed → _state=.idle
        q.deinit();
        return ProposalValidationError.ProposalWouldNotCompile;
    };
    // Rule 4: dupe before draining — row data lives in the connection reader buffer
    const raw_json = alloc.dupe(u8, try row.get([]const u8, 0)) catch |err| {
        q.drain() catch {};
        q.deinit();
        return err;
    };
    defer alloc.free(raw_json);
    q.drain() catch {}; // Rule 2: drain 'C'+'Z' → _state=.idle
    q.deinit();
    return topology.parseProfileJson(alloc, raw_json) catch ProposalValidationError.ProposalWouldNotCompile;
}

fn validateCandidateProfileSkills(
    conn: *pg.Conn,
    workspace_id: []const u8,
    profile: *const topology.Profile,
) ProposalValidationError!void {
    for (profile.stages) |stage| {
        try validateSkillRef(conn, workspace_id, stage.skill_id);
    }
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

fn agentExistsInWorkspace(conn: *pg.Conn, workspace_id: []const u8, agent_id: []const u8) bool {
    var q = conn.query(
        \\SELECT 1
        \\FROM agent_profiles
        \\WHERE workspace_id = $1 AND agent_id = $2
        \\LIMIT 1
    , .{ workspace_id, agent_id }) catch return false;
    const found = (q.next() catch null) != null;
    if (found) q.drain() catch {}; // Rule 3: drain when row found before early exit
    q.deinit();
    return found;
}

fn workspaceAllowsCustomSkills(conn: *pg.Conn, workspace_id: []const u8) bool {
    var q = conn.query(
        \\SELECT allow_custom_skills
        \\FROM workspace_entitlements
        \\WHERE workspace_id = $1
        \\LIMIT 1
    , .{workspace_id}) catch return false;
    const row = (q.next() catch null) orelse {
        // null → 'C'+'Z' already consumed → _state=.idle
        q.deinit();
        return false;
    };
    const result = row.get(bool, 0) catch false;
    q.drain() catch {}; // Rule 2: drain remaining 'C'+'Z' → _state=.idle
    q.deinit();
    return result;
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

pub fn stageFromProposalValue(alloc: std.mem.Allocator, proposed_obj: std.json.ObjectMap) (ProposalValidationError || anyerror)!topology.Stage {
    const stage_id = stringField(proposed_obj, shared.JSON_KEY_STAGE_ID) orelse return ProposalValidationError.MissingStageId;
    const role = stringField(proposed_obj, shared.JSON_KEY_ROLE) orelse return ProposalValidationError.MissingRole;
    const skill = stringField(proposed_obj, shared.JSON_KEY_SKILL) orelse stringField(proposed_obj, shared.JSON_KEY_SKILL_ID) orelse return ProposalValidationError.InvalidSkillRef;
    return .{
        .stage_id = try alloc.dupe(u8, stage_id),
        .role_id = try alloc.dupe(u8, role),
        .skill_id = try alloc.dupe(u8, skill),
        .artifact_name = try alloc.dupe(u8, stringField(proposed_obj, shared.JSON_KEY_ARTIFACT_NAME) orelse "output.md"),
        .commit_message = try alloc.dupe(u8, stringField(proposed_obj, shared.JSON_KEY_COMMIT_MESSAGE) orelse "agent: add output.md"),
        .is_gate = boolField(proposed_obj, shared.JSON_KEY_GATE) orelse false,
        .on_pass = if (stringField(proposed_obj, shared.JSON_KEY_ON_PASS)) |value| try alloc.dupe(u8, value) else null,
        .on_fail = if (stringField(proposed_obj, shared.JSON_KEY_ON_FAIL)) |value| try alloc.dupe(u8, value) else null,
    };
}

pub fn stringifyProfileJson(alloc: std.mem.Allocator, profile: *const topology.Profile) ![]u8 {
    const StageDoc = struct {
        stage_id: []const u8,
        role: []const u8,
        skill: []const u8,
        artifact_name: []const u8,
        commit_message: []const u8,
        gate: bool,
        on_pass: ?[]const u8,
        on_fail: ?[]const u8,
    };
    const ProfileDoc = struct {
        agent_id: []const u8,
        stages: []const StageDoc,
    };

    var out: std.ArrayList(StageDoc) = .{};
    defer out.deinit(alloc);
    for (profile.stages) |stage| {
        try out.append(alloc, .{
            .stage_id = stage.stage_id,
            .role = stage.role_id,
            .skill = stage.skill_id,
            .artifact_name = stage.artifact_name,
            .commit_message = stage.commit_message,
            .gate = stage.is_gate,
            .on_pass = stage.on_pass,
            .on_fail = stage.on_fail,
        });
    }

    return std.json.Stringify.valueAlloc(alloc, ProfileDoc{
        .agent_id = profile.agent_id,
        .stages = out.items,
    }, .{});
}

pub fn cloneStage(alloc: std.mem.Allocator, stage: topology.Stage) !topology.Stage {
    return .{
        .stage_id = try alloc.dupe(u8, stage.stage_id),
        .role_id = try alloc.dupe(u8, stage.role_id),
        .skill_id = try alloc.dupe(u8, stage.skill_id),
        .artifact_name = try alloc.dupe(u8, stage.artifact_name),
        .commit_message = try alloc.dupe(u8, stage.commit_message),
        .is_gate = stage.is_gate,
        .on_pass = if (stage.on_pass) |value| try alloc.dupe(u8, value) else null,
        .on_fail = if (stage.on_fail) |value| try alloc.dupe(u8, value) else null,
    };
}

pub fn freeStage(alloc: std.mem.Allocator, stage: topology.Stage) void {
    alloc.free(stage.stage_id);
    alloc.free(stage.role_id);
    alloc.free(stage.skill_id);
    alloc.free(stage.artifact_name);
    alloc.free(stage.commit_message);
    if (stage.on_pass) |value| alloc.free(value);
    if (stage.on_fail) |value| alloc.free(value);
}

pub fn indexOfStage(stages: []const topology.Stage, stage_id: []const u8) ?usize {
    for (stages, 0..) |stage, idx| {
        if (std.mem.eql(u8, stage.stage_id, stage_id)) return idx;
    }
    return null;
}

fn stringField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const value = obj.get(key) orelse return null;
    return switch (value) {
        .string => |raw| raw,
        else => null,
    };
}

fn boolField(obj: std.json.ObjectMap, key: []const u8) ?bool {
    const value = obj.get(key) orelse return null;
    return switch (value) {
        .bool => |raw| raw,
        else => null,
    };
}
