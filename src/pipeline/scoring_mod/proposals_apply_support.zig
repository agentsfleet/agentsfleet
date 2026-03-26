const std = @import("std");
const pg = @import("pg");
const id_format = @import("../../types/id_format.zig");
const shared = @import("proposals_shared.zig");
const validation = @import("proposals_validation.zig");

pub const ProposalAutoApprovalError = error{
    MissingConfigVersionContext,
    ActivateTargetMissing,
    ProposalNotApproved,
};

pub const ChangeLogEntry = struct {
    field_name: []u8,
    old_value: []u8,
    new_value: []u8,

    pub fn deinit(self: *ChangeLogEntry, alloc: std.mem.Allocator) void {
        alloc.free(self.field_name);
        alloc.free(self.old_value);
        alloc.free(self.new_value);
    }
};

pub fn collectChangeLogEntries(alloc: std.mem.Allocator, proposed_changes: []const u8) ![]ChangeLogEntry {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, proposed_changes, .{});
    defer parsed.deinit();

    const items = switch (parsed.value) {
        .array => |value| value.items,
        else => return shared.ProposalError.ProposalNotArray,
    };

    var list: std.ArrayList(ChangeLogEntry) = .{};
    errdefer {
        for (list.items) |*entry| entry.deinit(alloc);
        list.deinit(alloc);
    }

    for (items) |item| {
        const obj = switch (item) {
            .object => |value| value,
            else => return shared.ProposalError.ProposalChangeNotObject,
        };
        const target_field = switch (obj.get(shared.JSON_KEY_TARGET_FIELD) orelse return shared.ProposalError.MissingTargetField) {
            .string => |value| value,
            else => return shared.ProposalError.MissingTargetField,
        };
        const proposed_value = obj.get(shared.JSON_KEY_PROPOSED_VALUE) orelse return shared.ProposalError.InvalidProposalJson;
        const current_value = obj.get(shared.JSON_KEY_CURRENT_VALUE) orelse std.json.Value{ .null = {} };

        try list.append(alloc, .{
            .field_name = try alloc.dupe(u8, target_field),
            .old_value = try std.json.Stringify.valueAlloc(alloc, current_value, .{}),
            .new_value = try std.json.Stringify.valueAlloc(alloc, proposed_value, .{}),
        });
    }

    return try list.toOwnedSlice(alloc);
}

pub fn loadCurrentActiveConfigVersionId(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !?[]u8 {
    var q = try conn.query(
        \\SELECT config_version_id::text
        \\FROM workspace_active_config
        \\WHERE workspace_id = $1
        \\LIMIT 1
    , .{workspace_id});

    const row = (try q.next()) orelse {
        q.deinit();
        return null;
    };
    const result = try alloc.dupe(u8, try row.get([]const u8, 0));
    q.drain() catch {};
    q.deinit();
    return result;
}

pub fn persistCandidateConfigVersion(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    candidate_profile_json: []const u8,
    now_ms: i64,
) ![]const u8 {
    const config_version_id = try id_format.generateTransitionId(alloc);
    errdefer alloc.free(config_version_id);

    var current_q = try conn.query(
        \\SELECT tenant_id, COALESCE(MAX(version), 0)::INTEGER
        \\FROM agent_config_versions
        \\WHERE agent_id = $1
        \\GROUP BY tenant_id
        \\ORDER BY MAX(version) DESC
        \\LIMIT 1
    , .{agent_id});

    const row = (try current_q.next()) orelse {
        current_q.deinit();
        return ProposalAutoApprovalError.MissingConfigVersionContext;
    };
    const tenant_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    defer alloc.free(tenant_id);
    const next_version = (try row.get(i32, 1)) + 1;
    current_q.drain() catch {};
    current_q.deinit();

    _ = try conn.exec(
        \\INSERT INTO agent_config_versions
        \\  (config_version_id, tenant_id, agent_id, version, source_markdown, compiled_profile_json,
        \\   compile_engine, validation_report_json, is_valid, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, TRUE, $9, $9)
    , .{
        config_version_id,
        tenant_id,
        agent_id,
        next_version,
        candidate_profile_json,
        candidate_profile_json,
        shared.COMPILE_ENGINE_DETERMINISTIC_V1,
        shared.VALIDATION_STATUS_AUTO_APPLIED_JSON,
        now_ms,
    });

    return config_version_id;
}

pub fn activateConfigVersion(
    conn: *pg.Conn,
    agent_id: []const u8,
    workspace_id: []const u8,
    activated_config_version_id: []const u8,
    applied_by: []const u8,
    now_ms: i64,
) !void {
    var q = try conn.query(
        \\UPDATE workspace_active_config
        \\SET config_version_id = $2,
        \\    activated_by = $3,
        \\    activated_at = $4
        \\WHERE workspace_id = $1
        \\RETURNING workspace_id
    , .{
        workspace_id,
        activated_config_version_id,
        applied_by,
        now_ms,
    });
    const updated = (try q.next()) != null;
    if (updated) q.drain() catch {};
    q.deinit();
    if (!updated) return ProposalAutoApprovalError.ActivateTargetMissing;

    _ = try conn.exec(
        \\UPDATE agent_profiles
        \\SET status = CASE WHEN agent_id = $1 THEN 'ACTIVE' ELSE status END,
        \\    updated_at = $2
        \\WHERE workspace_id = $3
    , .{ agent_id, now_ms, workspace_id });
}

pub fn activateAppliedProposal(
    conn: *pg.Conn,
    agent_id: []const u8,
    workspace_id: []const u8,
    proposal_id: []const u8,
    activated_config_version_id: []const u8,
    applied_by: []const u8,
    now_ms: i64,
) !void {
    try activateConfigVersion(conn, agent_id, workspace_id, activated_config_version_id, applied_by, now_ms);

    var q = try conn.query(
        \\UPDATE agent_improvement_proposals
        \\SET status = $2,
        \\    applied_by = $3,
        \\    updated_at = $4
        \\WHERE proposal_id = $1
        \\  AND status = $5
        \\RETURNING proposal_id
    , .{
        proposal_id,
        shared.STATUS_APPLIED,
        applied_by,
        now_ms,
        shared.STATUS_APPROVED,
    });
    const updated = (try q.next()) != null;
    if (updated) q.drain() catch {};
    q.deinit();
    if (!updated) return ProposalAutoApprovalError.ProposalNotApproved;
}

pub fn insertHarnessChangeLog(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    proposal_id: []const u8,
    workspace_id: []const u8,
    entries: []const ChangeLogEntry,
    applied_by: []const u8,
    now_ms: i64,
) !void {
    for (entries) |entry| {
        const change_id = try id_format.generateTransitionId(alloc);
        defer alloc.free(change_id);

        _ = try conn.exec(
            \\INSERT INTO harness_change_log
            \\  (change_id, agent_id, proposal_id, workspace_id, field_name, old_value, new_value, applied_at, applied_by, reverted_from)
            \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NULL)
        , .{
            change_id,
            agent_id,
            proposal_id,
            workspace_id,
            entry.field_name,
            entry.old_value,
            entry.new_value,
            now_ms,
            applied_by,
        });
    }
}

pub fn markProposalApprovedIfExpected(
    conn: *pg.Conn,
    agent_id: []const u8,
    workspace_id: []const u8,
    proposal_id: []const u8,
    config_version_id: []const u8,
    proposed_changes: []const u8,
    expected_status: []const u8,
    now_ms: i64,
) !bool {
    var q = try conn.query(
        \\UPDATE agent_improvement_proposals
        \\SET status = $2,
        \\    updated_at = $3
        \\WHERE proposal_id = $1
        \\  AND agent_id = $4
        \\  AND workspace_id = $5
        \\  AND config_version_id = $6
        \\  AND proposed_changes = $7
        \\  AND status = $8
        \\RETURNING proposal_id
    , .{
        proposal_id,
        shared.STATUS_APPROVED,
        now_ms,
        agent_id,
        workspace_id,
        config_version_id,
        proposed_changes,
        expected_status,
    });

    const changed = (try q.next()) != null;
    if (changed) q.drain() catch {};
    q.deinit();
    return changed;
}

pub fn ensureProposalApproved(conn: *pg.Conn, proposal_id: []const u8) !bool {
    var q = try conn.query(
        \\SELECT 1
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = $1
        \\  AND status = $2
        \\LIMIT 1
    , .{ proposal_id, shared.STATUS_APPROVED });

    const found = (try q.next()) != null;
    if (found) q.drain() catch {};
    q.deinit();
    return found;
}

pub fn markProposalConfigChanged(conn: *pg.Conn, proposal_id: []const u8, now_ms: i64) !void {
    _ = try conn.exec(
        \\UPDATE agent_improvement_proposals
        \\SET status = $2,
        \\    rejection_reason = $3,
        \\    updated_at = $4
        \\WHERE proposal_id = $1
    , .{
        proposal_id,
        shared.STATUS_CONFIG_CHANGED,
        shared.REJECTION_REASON_CONFIG_CHANGED_SINCE_PROPOSAL,
        now_ms,
    });
}

pub fn rejectProposal(conn: *pg.Conn, proposal_id: []const u8, rejection_reason: []const u8, now_ms: i64) !void {
    _ = try conn.exec(
        \\UPDATE agent_improvement_proposals
        \\SET status = $2,
        \\    rejection_reason = $3,
        \\    updated_at = $4
        \\WHERE proposal_id = $1
    , .{
        proposal_id,
        shared.STATUS_REJECTED,
        rejection_reason,
        now_ms,
    });
}

pub fn expireStaleManualProposals(conn: *pg.Conn, cutoff_ms: i64) !u32 {
    // check-pg-drain: ok — full while loop exhausts all rows, natural drain
    var q = try conn.query(
        \\UPDATE agent_improvement_proposals
        \\SET status = $1,
        \\    rejection_reason = $2,
        \\    updated_at = $3
        \\WHERE approval_mode = $4
        \\  AND generation_status = $5
        \\  AND status = $6
        \\  AND created_at <= $7
        \\RETURNING proposal_id
    , .{
        shared.STATUS_REJECTED,
        shared.REJECTION_REASON_EXPIRED,
        std.time.milliTimestamp(),
        shared.ApprovalMode.manual.label(),
        shared.GENERATION_STATUS_READY,
        shared.STATUS_PENDING_REVIEW,
        cutoff_ms,
    });

    var expired: u32 = 0;
    while (try q.next()) |_| expired += 1;
    q.deinit();
    return expired;
}
