const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const error_codes = @import("../errors/error_registry.zig");
const topology = @import("topology.zig");
const id_format = @import("../types/id_format.zig");

const log = std.log.scoped(.state);

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

fn loadPolicy(conn: *pg.Conn, workspace_id: []const u8) !EntitlementPolicy {
    var q = PgQuery.from(try conn.query(
        \\SELECT plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills
        \\FROM workspace_entitlements
        \\WHERE workspace_id = $1
        \\LIMIT 1
    , .{workspace_id}));
    defer q.deinit();

    const row = (try q.next()) orelse return EnforcementError.EntitlementMissing;
    // Read all column values before drain — row buffer lives in conn reader.
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
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::BIGINT FROM agent_profiles WHERE workspace_id = $1",
        .{workspace_id},
    ));
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

    // default_prof must be declared BEFORE default_skill_ids so Zig's LIFO defer
    // order frees the HashMap first (releasing its structure) then the profile
    // (releasing the skill_id strings that serve as borrowed keys).
    var default_prof_opt: ?topology.Profile = null;
    defer if (default_prof_opt) |*p| p.deinit();
    var default_skill_ids = std.StringHashMap(void).init(alloc);
    defer default_skill_ids.deinit();
    if (!policy.allow_custom_skills) {
        default_prof_opt = try topology.defaultProfile(alloc);
        for (default_prof_opt.?.stages) |ds| try default_skill_ids.put(ds.skill_id, {});
    }

    var distinct_skills = std.StringHashMap(void).init(alloc);
    defer distinct_skills.deinit();
    for (profile.stages) |stage| {
        try distinct_skills.put(stage.skill_id, {});
        if (!policy.allow_custom_skills and !default_skill_ids.contains(stage.skill_id)) {
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

    // Rule 1: exec() for INSERT — internal drain loop, always leaves _state=.idle
    _ = try conn.exec(
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
            log.debug("entitlement.deny workspace_id={s} boundary={s} reason=missing", .{ workspace_id, boundary.label() });
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

    log.debug("entitlement.allow workspace_id={s} boundary={s} tier={s} profiles={d} stages={d}", .{ workspace_id, boundary.label(), policy.tier.label(), observed.profile_count, observed.stage_count });
    try insertAuditSnapshot(conn, alloc, workspace_id, boundary, "ALLOW", "ALLOW", policy.tier, policy, observed, actor);
}

test "unit: evaluateProfile rejects disallowed skill with stable reason code" {
    const raw =
        \\{
        \\  "agent_id":"prof_1",
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
        \\  "agent_id":"prof_1",
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

// --- T6: Integration — evaluateProfile ALLOW paths ---

test "T6: evaluateProfile returns null (ALLOW) for SCALE tier with custom skill" {
    const raw =
        \\{
        \\  "agent_id":"prof_scale",
        \\  "stages":[
        \\    {"stage_id":"plan","role":"planner","skill":"echo"},
        \\    {"stage_id":"implement","role":"coder","skill":"custom_skill"},
        \\    {"stage_id":"verify","role":"reviewer","skill":"warden","gate":true,"on_pass":"done","on_fail":"retry"}
        \\  ]
        \\}
    ;
    var observed: Observed = .{};
    const reason = try evaluateProfile(std.testing.allocator, .{
        .tier = .scale,
        .max_profiles = 5,
        .max_stages = 10,
        .max_distinct_skills = 10,
        .allow_custom_skills = true,
    }, raw, &observed);
    try std.testing.expectEqual(@as(?[]const u8, null), reason);
    try std.testing.expectEqual(@as(u16, 3), observed.stage_count);
    try std.testing.expectEqual(@as(u16, 3), observed.distinct_skill_count);
}

test "T6: evaluateProfile returns null (ALLOW) when compiled_profile_json is null" {
    var observed: Observed = .{};
    const reason = try evaluateProfile(std.testing.allocator, .{
        .tier = .free,
        .max_profiles = 1,
        .max_stages = 3,
        .max_distinct_skills = 3,
        .allow_custom_skills = false,
    }, null, &observed);
    // No profile to evaluate — allow (caller controls whether null is valid in context)
    try std.testing.expectEqual(@as(?[]const u8, null), reason);
}

test "T6: evaluateProfile reports correct distinct_skill_count with repeated skills" {
    const raw =
        \\{
        \\  "agent_id":"dup-skills",
        \\  "stages":[
        \\    {"stage_id":"plan","role":"echo","skill":"echo"},
        \\    {"stage_id":"implement","role":"scout","skill":"scout"},
        \\    {"stage_id":"extra","role":"scout2","skill":"scout"},
        \\    {"stage_id":"verify","role":"warden","skill":"warden","gate":true,"on_pass":"done","on_fail":"retry"}
        \\  ]
        \\}
    ;
    var observed: Observed = .{};
    _ = try evaluateProfile(std.testing.allocator, .{
        .tier = .free,
        .max_profiles = 1,
        .max_stages = 4,
        .max_distinct_skills = 3,
        .allow_custom_skills = false,
    }, raw, &observed);
    // echo + scout + warden = 3 distinct (scout appears twice but counts once)
    try std.testing.expectEqual(@as(u16, 3), observed.distinct_skill_count);
}

// --- T8: OWASP Agent Security — entitlement guard after M20_001 isCoreSkill() removal ---

test "T8: free tier ALLOWS all three default skills (invariant before and after M20_001)" {
    // This invariant MUST hold before and after M20_001 removes isCoreSkill():
    // a FREE workspace with allow_custom_skills=false must allow echo/scout/warden.
    const raw =
        \\{
        \\  "agent_id":"free-default",
        \\  "stages":[
        \\    {"stage_id":"plan","role":"echo","skill":"echo"},
        \\    {"stage_id":"implement","role":"scout","skill":"scout"},
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
    try std.testing.expectEqual(@as(?[]const u8, null), reason);
}

test "T8: free tier DENIES when custom skill is added alongside default skills" {
    // Regression: adding one custom skill to a free-tier profile must be rejected,
    // even if all other skills are default built-ins.
    const raw =
        \\{
        \\  "agent_id":"free-plus-custom",
        \\  "stages":[
        \\    {"stage_id":"plan","role":"echo","skill":"echo"},
        \\    {"stage_id":"implement","role":"scout","skill":"custom-injection-skill"},
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

test "T8: free tier DENIES skill_id with surrounding whitespace (no bypass via padding)" {
    // "echo " (trailing space) is NOT equal to "echo" under eqlIgnoreCase (length mismatch).
    // This pins the behavior: no whitespace normalization bypass in isCoreSkill.
    const raw =
        \\{
        \\  "agent_id":"ws-bypass",
        \\  "stages":[
        \\    {"stage_id":"plan","role":"echo","skill":"echo "},
        \\    {"stage_id":"implement","role":"scout","skill":"scout"},
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
    // "echo " != "echo" → treated as custom skill → denied on free tier
    try std.testing.expect(reason != null);
    try std.testing.expectEqualStrings(error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, reason.?);
}

test "M20_001 T1: SCALE tier with allow_custom_skills=true ALLOWS clawhub:// skill" {
    const raw =
        \\{"agent_id":"sc","stages":[
        \\  {"stage_id":"plan","role":"r","skill":"echo"},
        \\  {"stage_id":"implement","role":"r","skill":"clawhub://usezombie/go-reviewer@1.0.0"},
        \\  {"stage_id":"verify","role":"r","skill":"warden","gate":true,"on_pass":"done","on_fail":"retry"}
        \\]}
    ;
    var observed: Observed = .{};
    const reason = try evaluateProfile(std.testing.allocator, .{
        .tier = .scale,
        .max_profiles = 10,
        .max_stages = 5,
        .max_distinct_skills = 5,
        .allow_custom_skills = true,
    }, raw, &observed);
    try std.testing.expectEqual(@as(?[]const u8, null), reason);
    try std.testing.expectEqual(@as(u16, 3), observed.distinct_skill_count);
}

test "M20_001 T6 integration: custom role_ids (planner/coder/reviewer) with default skills ALLOWED on free tier" {
    // AC 5.5: skills matter for entitlement checks, not role names.
    const raw =
        \\{"agent_id":"cr","stages":[
        \\  {"stage_id":"plan","role":"planner","skill":"echo"},
        \\  {"stage_id":"implement","role":"coder","skill":"scout"},
        \\  {"stage_id":"verify","role":"reviewer","skill":"warden","gate":true,"on_pass":"done","on_fail":"retry"}
        \\]}
    ;
    var observed: Observed = .{};
    const reason = try evaluateProfile(std.testing.allocator, .{
        .tier = .free,
        .max_profiles = 1,
        .max_stages = 3,
        .max_distinct_skills = 3,
        .allow_custom_skills = false,
    }, raw, &observed);
    try std.testing.expectEqual(@as(?[]const u8, null), reason);
}

test "M20_001 T6 integration: custom skill DENIED on free tier" {
    const raw =
        \\{"agent_id":"ms","stages":[{"stage_id":"plan","role":"planner","skill":"echo"},{"stage_id":"implement","role":"coder","skill":"custom-analyzer"},{"stage_id":"verify","role":"reviewer","skill":"warden","gate":true,"on_pass":"done","on_fail":"retry"}]}
    ;
    var observed: Observed = .{};
    const reason = try evaluateProfile(std.testing.allocator, .{ .tier = .free, .max_profiles = 1, .max_stages = 3, .max_distinct_skills = 3, .allow_custom_skills = false }, raw, &observed);
    try std.testing.expect(reason != null);
    try std.testing.expectEqualStrings(error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, reason.?);
}
