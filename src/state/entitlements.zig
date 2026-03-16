const std = @import("std");
const pg = @import("pg");
const error_codes = @import("../errors/codes.zig");
const topology = @import("../pipeline/topology.zig");
const id_format = @import("../types/id_format.zig");

pub const Boundary = enum {
    compile,
    activate,
    runtime,

    fn label(self: Boundary) []const u8 {
        return switch (self) {
            .compile => "COMPILE",
            .activate => "ACTIVATE",
            .runtime => "RUNTIME",
        };
    }
};

pub const PolicyTier = enum {
    free,
    scale,
    unknown,

    fn label(self: PolicyTier) []const u8 {
        return switch (self) {
            .free => "FREE",
            .scale => "SCALE",
            .unknown => "UNKNOWN",
        };
    }
};

pub const EntitlementPolicy = struct {
    tier: PolicyTier,
    max_profiles: u16,
    max_stages: u16,
    max_distinct_skills: u16,
    allow_custom_skills: bool,
};

pub const Observed = struct {
    profile_count: u32 = 0,
    stage_count: u16 = 0,
    distinct_skill_count: u16 = 0,
    config_version_id: ?[]const u8 = null,
};

pub const EnforcementError = error{
    EntitlementMissing,
    EntitlementProfileLimit,
    EntitlementStageLimit,
    EntitlementSkillNotAllowed,
    InvalidCompiledProfile,
};

fn parseTier(raw: []const u8) ?PolicyTier {
    if (std.ascii.eqlIgnoreCase(raw, "FREE")) return .free;
    if (std.ascii.eqlIgnoreCase(raw, "SCALE")) return .scale;
    return null;
}

fn isCoreSkill(skill_id: []const u8) bool {
    return std.ascii.eqlIgnoreCase(skill_id, topology.ROLE_ECHO) or
        std.ascii.eqlIgnoreCase(skill_id, topology.ROLE_SCOUT) or
        std.ascii.eqlIgnoreCase(skill_id, topology.ROLE_WARDEN);
}

fn loadPolicy(conn: *pg.Conn, workspace_id: []const u8) !EntitlementPolicy {
    var q = try conn.query(
        \\SELECT plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills
        \\FROM workspace_entitlements
        \\WHERE workspace_id = $1
        \\LIMIT 1
    , .{workspace_id});
    defer q.deinit();

    const row = (try q.next()) orelse return EnforcementError.EntitlementMissing;
    const tier_raw = try row.get([]const u8, 0);
    const tier = parseTier(tier_raw) orelse return EnforcementError.EntitlementMissing;
    const max_profiles_i32 = try row.get(i32, 1);
    const max_stages_i32 = try row.get(i32, 2);
    const max_distinct_skills_i32 = try row.get(i32, 3);
    const allow_custom_skills = try row.get(bool, 4);
    if (max_profiles_i32 <= 0 or max_stages_i32 <= 0 or max_distinct_skills_i32 <= 0) {
        return EnforcementError.EntitlementMissing;
    }

    return .{
        .tier = tier,
        .max_profiles = @intCast(max_profiles_i32),
        .max_stages = @intCast(max_stages_i32),
        .max_distinct_skills = @intCast(max_distinct_skills_i32),
        .allow_custom_skills = allow_custom_skills,
    };
}

fn countWorkspaceProfiles(conn: *pg.Conn, workspace_id: []const u8) !u32 {
    var q = try conn.query(
        "SELECT COUNT(*)::BIGINT FROM agent_profiles WHERE workspace_id = $1",
        .{workspace_id},
    );
    defer q.deinit();

    const row = (try q.next()) orelse return 0;
    const count = try row.get(i64, 0);
    if (count <= 0) return 0;
    return @intCast(count);
}

fn evaluateProfile(
    alloc: std.mem.Allocator,
    policy: EntitlementPolicy,
    compiled_profile_json: ?[]const u8,
    observed: *Observed,
) !?[]const u8 {
    const raw = compiled_profile_json orelse return null;
    var profile = topology.parseProfileJson(alloc, raw) catch return EnforcementError.InvalidCompiledProfile;
    defer profile.deinit();

    observed.stage_count = @intCast(profile.stages.len);
    if (profile.stages.len > policy.max_stages) return error_codes.ERR_ENTITLEMENT_STAGE_LIMIT;

    var distinct_skills = std.StringHashMap(void).init(alloc);
    defer distinct_skills.deinit();

    for (profile.stages) |stage| {
        try distinct_skills.put(stage.skill_id, {});
        if (!policy.allow_custom_skills and !isCoreSkill(stage.skill_id)) {
            return error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED;
        }
    }

    observed.distinct_skill_count = @intCast(distinct_skills.count());
    if (distinct_skills.count() > policy.max_distinct_skills) return error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED;
    return null;
}

fn insertAuditSnapshot(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    boundary: Boundary,
    decision: []const u8,
    reason_code: []const u8,
    policy_tier: PolicyTier,
    policy: ?EntitlementPolicy,
    observed: Observed,
    actor: []const u8,
) !void {
    const policy_json = if (policy) |p|
        try std.json.Stringify.valueAlloc(alloc, .{
            .plan_tier = p.tier.label(),
            .max_profiles = p.max_profiles,
            .max_stages = p.max_stages,
            .max_distinct_skills = p.max_distinct_skills,
            .allow_custom_skills = p.allow_custom_skills,
        }, .{})
    else
        try alloc.dupe(u8, "{}");
    defer alloc.free(policy_json);

    const observed_json = try std.json.Stringify.valueAlloc(alloc, .{
        .profile_count = observed.profile_count,
        .stage_count = observed.stage_count,
        .distinct_skill_count = observed.distinct_skill_count,
        .config_version_id = observed.config_version_id,
    }, .{});
    defer alloc.free(observed_json);

    const snapshot_id = try id_format.generateEntitlementSnapshotId(alloc);
    defer alloc.free(snapshot_id);

    var q = try conn.query(
        \\INSERT INTO entitlement_policy_audit_snapshots
        \\  (snapshot_id, workspace_id, boundary, decision, reason_code, plan_tier, policy_json, observed_json, actor, created_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $9, $10)
    , .{
        snapshot_id,
        workspace_id,
        boundary.label(),
        decision,
        reason_code,
        policy_tier.label(),
        policy_json,
        observed_json,
        actor,
        std.time.milliTimestamp(),
    });
    q.deinit();
}

pub fn enforceWithAudit(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    config_version_id: ?[]const u8,
    compiled_profile_json: ?[]const u8,
    boundary: Boundary,
    actor: []const u8,
) (EnforcementError || anyerror)!void {
    var observed: Observed = .{
        .config_version_id = config_version_id,
    };

    const policy = loadPolicy(conn, workspace_id) catch |err| {
        if (err == EnforcementError.EntitlementMissing) {
            try insertAuditSnapshot(conn, alloc, workspace_id, boundary, "DENY", error_codes.ERR_ENTITLEMENT_UNAVAILABLE, .unknown, null, observed, actor);
            return EnforcementError.EntitlementMissing;
        }
        return err;
    };

    observed.profile_count = try countWorkspaceProfiles(conn, workspace_id);
    if (observed.profile_count > policy.max_profiles) {
        try insertAuditSnapshot(conn, alloc, workspace_id, boundary, "DENY", error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT, policy.tier, policy, observed, actor);
        return EnforcementError.EntitlementProfileLimit;
    }

    if (try evaluateProfile(alloc, policy, compiled_profile_json, &observed)) |reason_code| {
        if (std.mem.eql(u8, reason_code, error_codes.ERR_ENTITLEMENT_STAGE_LIMIT)) {
            try insertAuditSnapshot(conn, alloc, workspace_id, boundary, "DENY", reason_code, policy.tier, policy, observed, actor);
            return EnforcementError.EntitlementStageLimit;
        }
        try insertAuditSnapshot(conn, alloc, workspace_id, boundary, "DENY", reason_code, policy.tier, policy, observed, actor);
        return EnforcementError.EntitlementSkillNotAllowed;
    }

    try insertAuditSnapshot(conn, alloc, workspace_id, boundary, "ALLOW", "ALLOW", policy.tier, policy, observed, actor);
}

test "unit: evaluateProfile rejects disallowed skill with stable reason code" {
    const raw =
        \\{
        \\  "profile_id":"prof_1",
        \\  "stages":[
        \\    {"stage_id":"plan","role":"echo","skill":"echo"},
        \\    {"stage_id":"implement","role":"scout","skill":"custom_skill"},
        \\    {"stage_id":"verify","role":"warden","skill":"warden","gate":true,"on_pass":"done","on_fail":"retry"}
        \\  ]
        \\}
    ;
    var observed: Observed = .{};
    const reason = try evaluateProfile(std.testing.allocator, .{
        .tier = .free,
        .max_profiles = 1,
        .max_stages = 3,
        .max_distinct_skills = 3,
        .allow_custom_skills = false,
    }, raw, &observed);
    try std.testing.expect(reason != null);
    try std.testing.expectEqualStrings(error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, reason.?);
}

test "unit: evaluateProfile rejects stage limits deterministically" {
    const raw =
        \\{
        \\  "profile_id":"prof_1",
        \\  "stages":[
        \\    {"stage_id":"plan","role":"echo","skill":"echo"},
        \\    {"stage_id":"implement","role":"scout","skill":"scout"},
        \\    {"stage_id":"extra","role":"scout","skill":"scout","gate":false},
        \\    {"stage_id":"verify","role":"warden","skill":"warden","gate":true,"on_pass":"done","on_fail":"retry"}
        \\  ]
        \\}
    ;
    var observed: Observed = .{};
    const reason = try evaluateProfile(std.testing.allocator, .{
        .tier = .free,
        .max_profiles = 1,
        .max_stages = 3,
        .max_distinct_skills = 3,
        .allow_custom_skills = false,
    }, raw, &observed);
    try std.testing.expect(reason != null);
    try std.testing.expectEqualStrings(error_codes.ERR_ENTITLEMENT_STAGE_LIMIT, reason.?);
}
