const std = @import("std");
const pg = @import("pg");
const id_format = @import("../../types/id_format.zig");
const topology = @import("../topology.zig");
const auto_approval = @import("proposals_auto_approval.zig");
const shared = @import("proposals_shared.zig");
const validation = @import("proposals_validation.zig");

const sql_rollback = "ROLLBACK";

const RevertError = error{
    CurrentConfigMissing,
};

const LoggedChange = struct {
    proposal_id: []u8,
    workspace_id: []u8,
    field_name: []u8,
    old_value: []u8,
    new_value: []u8,

    fn deinit(self: *LoggedChange, alloc: std.mem.Allocator) void {
        alloc.free(self.proposal_id);
        alloc.free(self.workspace_id);
        alloc.free(self.field_name);
        alloc.free(self.old_value);
        alloc.free(self.new_value);
    }
};

pub const RevertHarnessResult = struct {
    change_id: []const u8,
    reverted_from: []const u8,
    proposal_id: []const u8,
    agent_id: []const u8,
    workspace_id: []const u8,
    config_version_id: []const u8,
    applied_by: []const u8,
    applied_at: i64,

    pub fn deinit(self: *RevertHarnessResult, alloc: std.mem.Allocator) void {
        alloc.free(self.change_id);
        alloc.free(self.reverted_from);
        alloc.free(self.proposal_id);
        alloc.free(self.agent_id);
        alloc.free(self.workspace_id);
        alloc.free(self.config_version_id);
        alloc.free(self.applied_by);
    }
};

pub fn revertHarnessChange(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    change_id: []const u8,
    operator_identity: []const u8,
    now_ms: i64,
) !?RevertHarnessResult {
    var logged_change = (try loadLoggedChange(conn, alloc, agent_id, change_id)) orelse return null;
    defer logged_change.deinit(alloc);

    const applied_by = try std.fmt.allocPrint(alloc, "{s}{s}", .{ shared.APPLIED_BY_OPERATOR_PREFIX, operator_identity });
    errdefer alloc.free(applied_by);

    _ = try conn.exec("BEGIN", .{});
    var tx_open = true;
    errdefer {
        if (tx_open) _ = conn.exec(sql_rollback, .{}) catch {};
    }

    const current_config_version_id = (try loadCurrentActiveConfigVersionId(conn, alloc, logged_change.workspace_id)) orelse {
        _ = conn.exec(sql_rollback, .{}) catch {};
        tx_open = false;
        return RevertError.CurrentConfigMissing;
    };
    defer alloc.free(current_config_version_id);

    const candidate_profile_json = try buildRevertedProfileJson(conn, alloc, current_config_version_id, logged_change);
    defer alloc.free(candidate_profile_json);

    const activated_config_version_id = try auto_approval.persistCandidateConfigVersion(
        conn,
        alloc,
        agent_id,
        candidate_profile_json,
        now_ms,
    );
    errdefer alloc.free(activated_config_version_id);

    try auto_approval.activateConfigVersion(
        conn,
        agent_id,
        logged_change.workspace_id,
        activated_config_version_id,
        applied_by,
        now_ms,
    );

    const revert_change_id = try id_format.generateTransitionId(alloc);
    errdefer alloc.free(revert_change_id);

    _ = try conn.exec(
        \\INSERT INTO harness_change_log
        \\  (change_id, agent_id, proposal_id, workspace_id, field_name, old_value, new_value, applied_at, applied_by, reverted_from)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
    , .{
        revert_change_id,
        agent_id,
        logged_change.proposal_id,
        logged_change.workspace_id,
        logged_change.field_name,
        logged_change.new_value,
        logged_change.old_value,
        now_ms,
        applied_by,
        change_id,
    });

    _ = try conn.exec("COMMIT", .{});
    tx_open = false;

    return .{
        .change_id = revert_change_id,
        .reverted_from = try alloc.dupe(u8, change_id),
        .proposal_id = try alloc.dupe(u8, logged_change.proposal_id),
        .agent_id = try alloc.dupe(u8, agent_id),
        .workspace_id = try alloc.dupe(u8, logged_change.workspace_id),
        .config_version_id = activated_config_version_id,
        .applied_by = applied_by,
        .applied_at = now_ms,
    };
}

fn loadLoggedChange(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    change_id: []const u8,
) !?LoggedChange {
    var q = try conn.query(
        \\SELECT proposal_id, workspace_id, field_name, old_value, new_value
        \\FROM harness_change_log
        \\WHERE change_id = $1
        \\  AND agent_id = $2
        \\LIMIT 1
    , .{ change_id, agent_id });

    const row = (try q.next()) orelse {
        q.deinit();
        return null;
    };
    const result = LoggedChange{
        .proposal_id = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .workspace_id = try alloc.dupe(u8, try row.get([]const u8, 1)),
        .field_name = try alloc.dupe(u8, try row.get([]const u8, 2)),
        .old_value = try alloc.dupe(u8, try row.get([]const u8, 3)),
        .new_value = try alloc.dupe(u8, try row.get([]const u8, 4)),
    };
    q.drain() catch {};
    q.deinit();
    return result;
}

fn loadCurrentActiveConfigVersionId(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !?[]u8 {
    var q = try conn.query(
        \\SELECT config_version_id
        \\FROM workspace_active_config
        \\WHERE workspace_id = $1
        \\LIMIT 1
    , .{workspace_id});

    const row = (try q.next()) orelse {
        q.deinit();
        return null;
    };
    const config_version_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    q.drain() catch {};
    q.deinit();
    return config_version_id;
}

fn loadConfigProfile(conn: *pg.Conn, alloc: std.mem.Allocator, config_version_id: []const u8) !topology.Profile {
    var q = try conn.query(
        \\SELECT compiled_profile_json
        \\FROM agent_config_versions
        \\WHERE config_version_id = $1
        \\LIMIT 1
    , .{config_version_id});
    const row = (try q.next()) orelse {
        q.deinit();
        return shared.ProposalError.ProposalWouldNotCompile;
    };
    const raw_json = try alloc.dupe(u8, try row.get([]const u8, 0));
    defer alloc.free(raw_json);
    q.drain() catch {};
    q.deinit();
    return topology.parseProfileJson(alloc, raw_json) catch shared.ProposalError.ProposalWouldNotCompile;
}

fn buildRevertedProfileJson(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    config_version_id: []const u8,
    logged_change: LoggedChange,
) ![]u8 {
    var profile = try loadConfigProfile(conn, alloc, config_version_id);
    defer profile.deinit();

    try applyRevert(alloc, &profile, logged_change);

    const candidate_profile_json = try validation.stringifyProfileJson(alloc, &profile);
    errdefer alloc.free(candidate_profile_json);

    var parsed = topology.parseProfileJson(alloc, candidate_profile_json) catch {
        return shared.ProposalError.ProposalWouldNotCompile;
    };
    parsed.deinit();

    return candidate_profile_json;
}

fn applyRevert(
    alloc: std.mem.Allocator,
    profile: *topology.Profile,
    logged_change: LoggedChange,
) !void {
    if (std.mem.eql(u8, logged_change.field_name, shared.PROPOSAL_TARGET_STAGE_INSERT)) {
        const stage_id = try extractStageId(alloc, logged_change.new_value);
        defer alloc.free(stage_id);
        const stage_index = validation.indexOfStage(profile.stages, stage_id) orelse {
            return shared.ProposalError.ProposalWouldNotCompile;
        };

        validation.freeStage(alloc, profile.stages[stage_index]);
        var idx = stage_index;
        while (idx + 1 < profile.stages.len) : (idx += 1) {
            profile.stages[idx] = profile.stages[idx + 1];
        }
        profile.stages = try alloc.realloc(profile.stages, profile.stages.len - 1);
        return;
    }

    if (std.mem.eql(u8, logged_change.field_name, shared.PROPOSAL_TARGET_STAGE_BINDING)) {
        const stage_id = try extractStageId(alloc, logged_change.old_value);
        defer alloc.free(stage_id);
        const replacement = try stageFromValueJson(alloc, logged_change.old_value);
        errdefer validation.freeStage(alloc, replacement);
        const stage_index = validation.indexOfStage(profile.stages, stage_id) orelse {
            return shared.ProposalError.ProposalWouldNotCompile;
        };

        validation.freeStage(alloc, profile.stages[stage_index]);
        profile.stages[stage_index] = replacement;
        return;
    }

    return shared.ProposalError.UnsupportedTargetField;
}

fn extractStageId(alloc: std.mem.Allocator, raw_json: []const u8) ![]u8 {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, raw_json, .{});
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |value| value,
        else => return shared.ProposalError.InvalidProposalJson,
    };
    const stage_id = switch (obj.get(shared.JSON_KEY_STAGE_ID) orelse return shared.ProposalError.MissingStageId) {
        .string => |value| value,
        else => return shared.ProposalError.MissingStageId,
    };
    return try alloc.dupe(u8, stage_id);
}

fn stageFromValueJson(alloc: std.mem.Allocator, raw_json: []const u8) !topology.Stage {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, raw_json, .{});
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |value| value,
        else => return shared.ProposalError.InvalidProposalJson,
    };
    return validation.stageFromProposalValue(alloc, obj);
}
