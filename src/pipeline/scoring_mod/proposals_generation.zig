const std = @import("std");
const pg = @import("pg");
const topology = @import("../topology.zig");
const shared = @import("proposals_shared.zig");

const log = std.log.scoped(.scoring);

pub fn generateProposalChanges(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    config_version_id: []const u8,
    trigger_reason: []const u8,
) (shared.ProposalError || anyerror)![]u8 {
    var profile = try loadConfigProfile(conn, alloc, config_version_id);
    defer profile.deinit();

    const gate_stage = profile.gateStage();
    const proposed_stage_id = try uniqueStageId(alloc, &profile, gate_stage.stage_id, "precheck");
    defer alloc.free(proposed_stage_id);
    const artifact_name = try std.fmt.allocPrint(alloc, "{s}.md", .{proposed_stage_id});
    defer alloc.free(artifact_name);
    const commit_message = try std.fmt.allocPrint(alloc, "agent: add {s}", .{artifact_name});
    defer alloc.free(commit_message);
    const rationale = try buildRationale(conn, alloc, agent_id, trigger_reason, gate_stage.skill_id);
    defer alloc.free(rationale);

    const ProposedValue = struct {
        agent_id: []const u8,
        insert_before_stage_id: []const u8,
        stage_id: []const u8,
        role: []const u8,
        skill: []const u8,
        artifact_name: []const u8,
        commit_message: []const u8,
        gate: bool,
        on_pass: ?[]const u8,
        on_fail: ?[]const u8,
    };
    const Change = struct {
        target_field: []const u8,
        current_value: ?ProposedValue,
        proposed_value: ProposedValue,
        rationale: []const u8,
    };

    log.info("proposal generated agent_id={s} stage_id={s} trigger={s}", .{ agent_id, proposed_stage_id, trigger_reason });

    return std.json.Stringify.valueAlloc(alloc, &[_]Change{.{
        .target_field = shared.PROPOSAL_TARGET_STAGE_INSERT,
        .current_value = null,
        .proposed_value = .{
            .agent_id = agent_id,
            .insert_before_stage_id = gate_stage.stage_id,
            .stage_id = proposed_stage_id,
            .role = gate_stage.role_id,
            .skill = gate_stage.skill_id,
            .artifact_name = artifact_name,
            .commit_message = commit_message,
            .gate = false,
            .on_pass = gate_stage.stage_id,
            .on_fail = topology.TRANSITION_RETRY,
        },
        .rationale = rationale,
    }}, .{});
}

fn buildRationale(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    trigger_reason: []const u8,
    review_skill: []const u8,
) ![]u8 {
    const dominant_failure = loadDominantFailureClass(conn, alloc, agent_id) catch null;
    defer if (dominant_failure) |value| alloc.free(value);
    if (dominant_failure) |failure_class| {
        return std.fmt.allocPrint(
            alloc,
            "{s} triggered after repeated {s} failures. Insert a precheck stage using pinned skill {s} before the gate stage so the harness can catch regressions earlier.",
            .{ trigger_reason, failure_class, review_skill },
        );
    }
    return std.fmt.allocPrint(
        alloc,
        "{s} triggered on recent score history. Insert a precheck stage before the gate stage using the current review skill {s} to tighten validation without changing the final gate contract.",
        .{ trigger_reason, review_skill },
    );
}

fn loadDominantFailureClass(conn: *pg.Conn, alloc: std.mem.Allocator, agent_id: []const u8) !?[]u8 {
    var q = try conn.query(
        \\SELECT failure_class, COUNT(*)::BIGINT AS failure_count
        \\FROM agent_run_analysis
        \\WHERE agent_id = $1 AND failure_class IS NOT NULL
        \\GROUP BY failure_class
        \\ORDER BY failure_count DESC, failure_class ASC
        \\LIMIT 1
    , .{agent_id});
    const row = (try q.next()) orelse {
        q.deinit();
        return null;
    };
    const result = try alloc.dupe(u8, try row.get([]const u8, 0));
    q.drain() catch {};
    q.deinit();
    return @as(?[]u8, result);
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
    const raw_json = alloc.dupe(u8, try row.get([]const u8, 0)) catch |err| {
        q.drain() catch {};
        q.deinit();
        return err;
    };
    defer alloc.free(raw_json);
    q.drain() catch {};
    q.deinit();
    return topology.parseProfileJson(alloc, raw_json) catch shared.ProposalError.ProposalWouldNotCompile;
}

fn uniqueStageId(
    alloc: std.mem.Allocator,
    profile: *const topology.Profile,
    base_stage_id: []const u8,
    suffix: []const u8,
) ![]u8 {
    const base = try std.fmt.allocPrint(alloc, "{s}-{s}", .{ base_stage_id, suffix });
    errdefer alloc.free(base);
    if (profile.indexOfStage(base) == null) return base;

    var idx: usize = 2;
    while (true) : (idx += 1) {
        const candidate = try std.fmt.allocPrint(alloc, "{s}-{d}", .{ base, idx });
        if (profile.indexOfStage(candidate) == null) {
            alloc.free(base);
            return candidate;
        }
        alloc.free(candidate);
    }
}
