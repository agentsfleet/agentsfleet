//! Pipeline topology profile loader.
//! Converts JSON config into a validated deterministic stage list.

const std = @import("std");

pub const ROLE_ECHO = "echo";
pub const ROLE_SCOUT = "scout";
pub const ROLE_WARDEN = "warden";

pub const STAGE_PLAN = "plan";
pub const STAGE_IMPLEMENT = "implement";
pub const STAGE_VERIFY = "verify";

pub const TRANSITION_DONE = "done";
pub const TRANSITION_RETRY = "retry";
pub const TRANSITION_BLOCKED = "blocked";

pub const TopologyError = error{
    InvalidProfile,
    MissingGateStage,
    GateStageMustBeLast,
    DuplicateStageId,
    InvalidTransitionTarget,
};

pub const Stage = struct {
    stage_id: []u8,
    role_id: []u8,
    skill_id: []u8,
    artifact_name: []u8,
    commit_message: []u8,
    is_gate: bool,
    on_pass: ?[]u8,
    on_fail: ?[]u8,
};

pub const Profile = struct {
    agent_id: []u8,
    stages: []Stage,
    alloc: std.mem.Allocator,

    pub fn deinit(self: *Profile) void {
        self.alloc.free(self.agent_id);
        freeStages(self.alloc, self.stages);
    }

    pub fn gateStage(self: *const Profile) Stage {
        return self.stages[self.stages.len - 1];
    }

    pub fn buildStages(self: *const Profile) []const Stage {
        return self.stages[1 .. self.stages.len - 1];
    }

    pub fn indexOfStage(self: *const Profile, stage_id: []const u8) ?usize {
        for (self.stages, 0..) |stage, idx| {
            if (std.mem.eql(u8, stage.stage_id, stage_id)) return idx;
        }
        return null;
    }
};

fn freeStages(alloc: std.mem.Allocator, stages: []Stage) void {
    for (stages) |stage| {
        alloc.free(stage.stage_id);
        alloc.free(stage.role_id);
        alloc.free(stage.skill_id);
        alloc.free(stage.artifact_name);
        alloc.free(stage.commit_message);
        if (stage.on_pass) |value| alloc.free(value);
        if (stage.on_fail) |value| alloc.free(value);
    }
    alloc.free(stages);
}

const StageDoc = struct {
    stage_id: []const u8,
    role: []const u8,
    skill: ?[]const u8 = null,
    artifact_name: ?[]const u8 = null,
    commit_message: ?[]const u8 = null,
    gate: ?bool = null,
    on_pass: ?[]const u8 = null,
    on_fail: ?[]const u8 = null,
};

const ProfileDoc = struct {
    agent_id: []const u8,
    stages: []const StageDoc,
};

pub fn parseProfileJson(alloc: std.mem.Allocator, raw: []const u8) !Profile {
    const parsed = try std.json.parseFromSlice(ProfileDoc, alloc, raw, .{});
    defer parsed.deinit();
    return fromDoc(alloc, parsed.value);
}

pub fn loadProfile(alloc: std.mem.Allocator, path: []const u8) !Profile {
    const file = std.fs.cwd().openFile(path, .{}) catch |err| switch (err) {
        error.FileNotFound => return defaultProfile(alloc),
        else => return err,
    };
    defer file.close();

    const raw = try file.readToEndAlloc(alloc, 256 * 1024);
    defer alloc.free(raw);

    return parseProfileJson(alloc, raw);
}

pub fn defaultProfile(alloc: std.mem.Allocator) !Profile {
    return Profile{
        .agent_id = try alloc.dupe(u8, "default-v1"),
        .stages = try alloc.dupe(Stage, &[_]Stage{
            .{
                .stage_id = try alloc.dupe(u8, STAGE_PLAN),
                .role_id = try alloc.dupe(u8, ROLE_ECHO),
                .skill_id = try alloc.dupe(u8, ROLE_ECHO),
                .artifact_name = try alloc.dupe(u8, "plan.json"),
                .commit_message = try alloc.dupe(u8, "echo: add plan.json"),
                .is_gate = false,
                .on_pass = null,
                .on_fail = null,
            },
            .{
                .stage_id = try alloc.dupe(u8, STAGE_IMPLEMENT),
                .role_id = try alloc.dupe(u8, ROLE_SCOUT),
                .skill_id = try alloc.dupe(u8, ROLE_SCOUT),
                .artifact_name = try alloc.dupe(u8, "implementation.md"),
                .commit_message = try alloc.dupe(u8, "scout: add implementation.md"),
                .is_gate = false,
                .on_pass = null,
                .on_fail = null,
            },
            .{
                .stage_id = try alloc.dupe(u8, STAGE_VERIFY),
                .role_id = try alloc.dupe(u8, ROLE_WARDEN),
                .skill_id = try alloc.dupe(u8, ROLE_WARDEN),
                .artifact_name = try alloc.dupe(u8, "validation.md"),
                .commit_message = try alloc.dupe(u8, "warden: add validation.md"),
                .is_gate = true,
                .on_pass = try alloc.dupe(u8, TRANSITION_DONE),
                .on_fail = try alloc.dupe(u8, TRANSITION_RETRY),
            },
        }),
        .alloc = alloc,
    };
}

fn fromDoc(alloc: std.mem.Allocator, doc: ProfileDoc) !Profile {
    if (doc.stages.len < 3) return TopologyError.InvalidProfile;

    const agent_id = try alloc.dupe(u8, doc.agent_id);
    errdefer alloc.free(agent_id);

    var stages: std.ArrayList(Stage) = .{};
    errdefer {
        for (stages.items) |stage| {
            alloc.free(stage.stage_id);
            alloc.free(stage.role_id);
            alloc.free(stage.skill_id);
            alloc.free(stage.artifact_name);
            alloc.free(stage.commit_message);
            if (stage.on_pass) |value| alloc.free(value);
            if (stage.on_fail) |value| alloc.free(value);
        }
        stages.deinit(alloc);
    }

    var seen_ids = std.StringHashMap(void).init(alloc);
    defer seen_ids.deinit();

    for (doc.stages, 0..) |stage_doc, idx| {
        if (stage_doc.stage_id.len == 0) return TopologyError.InvalidProfile;
        if (stage_doc.role.len == 0) return TopologyError.InvalidProfile;

        if (seen_ids.contains(stage_doc.stage_id)) return TopologyError.DuplicateStageId;
        try seen_ids.put(stage_doc.stage_id, {});

        const skill = stage_doc.skill orelse stage_doc.role;
        if (skill.len == 0) return TopologyError.InvalidProfile;

        const artifact_name = try alloc.dupe(u8, stage_doc.artifact_name orelse defaultArtifactName(skill));
        errdefer alloc.free(artifact_name);

        const commit_message = try alloc.dupe(u8, stage_doc.commit_message orelse defaultCommitMessage(stage_doc.role, skill));
        errdefer alloc.free(commit_message);

        const is_gate = stage_doc.gate orelse (idx == doc.stages.len - 1);

        try stages.append(alloc, .{
            .stage_id = try alloc.dupe(u8, stage_doc.stage_id),
            .role_id = try alloc.dupe(u8, stage_doc.role),
            .skill_id = try alloc.dupe(u8, skill),
            .artifact_name = artifact_name,
            .commit_message = commit_message,
            .is_gate = is_gate,
            .on_pass = if (stage_doc.on_pass) |target| try alloc.dupe(u8, target) else null,
            .on_fail = if (stage_doc.on_fail) |target| try alloc.dupe(u8, target) else null,
        });
    }

    const built = try stages.toOwnedSlice(alloc);
    errdefer freeStages(alloc, built);

    try validateProfile(built);

    return .{
        .agent_id = agent_id,
        .stages = built,
        .alloc = alloc,
    };
}

fn defaultArtifactName(skill: []const u8) []const u8 {
    if (std.ascii.eqlIgnoreCase(skill, ROLE_ECHO)) return "plan.json";
    if (std.ascii.eqlIgnoreCase(skill, ROLE_SCOUT)) return "implementation.md";
    if (std.ascii.eqlIgnoreCase(skill, ROLE_WARDEN)) return "validation.md";
    return "output.md";
}

fn defaultCommitMessage(role_id: []const u8, skill: []const u8) []const u8 {
    if (std.ascii.eqlIgnoreCase(skill, ROLE_ECHO)) return "echo: add plan.json";
    if (std.ascii.eqlIgnoreCase(skill, ROLE_SCOUT)) return "scout: add implementation.md";
    if (std.ascii.eqlIgnoreCase(skill, ROLE_WARDEN)) return "warden: add validation.md";
    _ = role_id;
    return "agent: add output.md";
}

fn validateProfile(stages: []const Stage) !void {
    if (stages.len < 3) return TopologyError.InvalidProfile;

    var gate_count: usize = 0;
    var gate_index: usize = 0;
    for (stages, 0..) |stage, i| {
        if (stage.is_gate) {
            gate_count += 1;
            gate_index = i;
        }
    }

    if (gate_count != 1) return TopologyError.MissingGateStage;
    if (gate_index != stages.len - 1) return TopologyError.GateStageMustBeLast;

    for (stages) |stage| {
        if (stage.on_pass) |target| {
            try validateTransitionTarget(stages, target);
        }
        if (stage.on_fail) |target| {
            try validateTransitionTarget(stages, target);
        }
    }
}

fn validateTransitionTarget(stages: []const Stage, target: []const u8) !void {
    if (std.ascii.eqlIgnoreCase(target, TRANSITION_DONE)) return;
    if (std.ascii.eqlIgnoreCase(target, TRANSITION_RETRY)) return;
    if (std.ascii.eqlIgnoreCase(target, TRANSITION_BLOCKED)) return;

    for (stages) |stage| {
        if (std.mem.eql(u8, stage.stage_id, target)) return;
    }

    return TopologyError.InvalidTransitionTarget;
}

test "default profile preserves v1 flow" {
    var profile = try defaultProfile(std.testing.allocator);
    defer profile.deinit();

    try std.testing.expectEqual(@as(usize, 3), profile.stages.len);
    try std.testing.expectEqualStrings(STAGE_PLAN, profile.stages[0].stage_id);
    try std.testing.expectEqualStrings(ROLE_ECHO, profile.stages[0].skill_id);
    try std.testing.expectEqualStrings(STAGE_IMPLEMENT, profile.stages[1].stage_id);
    try std.testing.expectEqualStrings(ROLE_SCOUT, profile.stages[1].skill_id);
    try std.testing.expectEqualStrings(STAGE_VERIFY, profile.stages[2].stage_id);
    try std.testing.expectEqualStrings(ROLE_WARDEN, profile.stages[2].skill_id);
    try std.testing.expectEqualStrings(TRANSITION_DONE, profile.stages[2].on_pass.?);
    try std.testing.expectEqualStrings(TRANSITION_RETRY, profile.stages[2].on_fail.?);
}

test "integration: custom profile with non built-in role and built-in skill is accepted" {
    const alloc = std.testing.allocator;
    const doc = ProfileDoc{
        .agent_id = "custom-profile",
        .stages = &[_]StageDoc{
            .{ .stage_id = "plan", .role = "echo" },
            .{ .stage_id = "security-review", .role = "security", .skill = "scout" },
            .{ .stage_id = "verify", .role = "warden", .gate = true, .on_pass = "done", .on_fail = "retry" },
        },
    };

    var profile = try fromDoc(alloc, doc);
    defer profile.deinit();

    try std.testing.expectEqualStrings("custom-profile", profile.agent_id);
    try std.testing.expectEqual(@as(usize, 3), profile.stages.len);
    try std.testing.expectEqualStrings("security", profile.stages[1].role_id);
    try std.testing.expectEqualStrings("scout", profile.stages[1].skill_id);
}

test "integration: stage transitions validate on_pass/on_fail targets" {
    const alloc = std.testing.allocator;
    const bad = ProfileDoc{
        .agent_id = "bad-transition",
        .stages = &[_]StageDoc{
            .{ .stage_id = "plan", .role = "echo" },
            .{ .stage_id = "implement", .role = "scout", .on_pass = "missing" },
            .{ .stage_id = "verify", .role = "warden", .gate = true },
        },
    };

    try std.testing.expectError(TopologyError.InvalidTransitionTarget, fromDoc(alloc, bad));
}

// --- T1: Happy path — loadProfile reads valid JSON from file ---

test "loadProfile reads valid profile from temp file" {
    const alloc = std.testing.allocator;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();

    const json =
        \\{"agent_id":"test-v1","stages":[
        \\  {"stage_id":"plan","role":"echo"},
        \\  {"stage_id":"implement","role":"scout"},
        \\  {"stage_id":"verify","role":"warden","gate":true,"on_pass":"done","on_fail":"retry"}
        \\]}
    ;
    try tmp.dir.writeFile(.{ .sub_path = "profile.json", .data = json });

    const path = try tmp.dir.realpathAlloc(alloc, "profile.json");
    defer alloc.free(path);

    var profile = try loadProfile(alloc, path);
    defer profile.deinit();
    try std.testing.expectEqualStrings("test-v1", profile.agent_id);
    try std.testing.expectEqual(@as(usize, 3), profile.stages.len);
    try std.testing.expectEqualStrings("plan", profile.stages[0].stage_id);
    try std.testing.expectEqualStrings("echo", profile.stages[0].role_id);
}

// --- T3: Error/fallback paths ---

test "loadProfile falls back to default when file not found" {
    var profile = try loadProfile(std.testing.allocator, "/tmp/nonexistent-zombie-test-profile-abc123.json");
    defer profile.deinit();
    try std.testing.expectEqualStrings("default-v1", profile.agent_id);
    try std.testing.expectEqual(@as(usize, 3), profile.stages.len);
}

test "loadProfile returns error on malformed JSON" {
    const alloc = std.testing.allocator;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();

    try tmp.dir.writeFile(.{ .sub_path = "bad.json", .data = "{invalid json" });

    const path = try tmp.dir.realpathAlloc(alloc, "bad.json");
    defer alloc.free(path);

    try std.testing.expectError(error.SyntaxError, loadProfile(alloc, path));
}

test "loadProfile returns error on too few stages" {
    const alloc = std.testing.allocator;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();

    const json =
        \\{"agent_id":"short","stages":[
        \\  {"stage_id":"plan","role":"echo"},
        \\  {"stage_id":"verify","role":"warden"}
        \\]}
    ;
    try tmp.dir.writeFile(.{ .sub_path = "short.json", .data = json });

    const path = try tmp.dir.realpathAlloc(alloc, "short.json");
    defer alloc.free(path);

    try std.testing.expectError(TopologyError.InvalidProfile, loadProfile(alloc, path));
}

// --- T7: Regression — loadProfile with cwd().openFile handles both path types ---

test "loadProfile default fallback preserves v1 contract" {
    // Regression: ensure the fallback default profile matches the expected v1 stages
    var profile = try loadProfile(std.testing.allocator, "/no/such/path.json");
    defer profile.deinit();
    try std.testing.expectEqualStrings(STAGE_PLAN, profile.stages[0].stage_id);
    try std.testing.expectEqualStrings(STAGE_IMPLEMENT, profile.stages[1].stage_id);
    try std.testing.expectEqualStrings(STAGE_VERIFY, profile.stages[2].stage_id);
    try std.testing.expect(profile.stages[2].is_gate);
}

test "profile with custom gate skill is accepted (roles are dynamic)" {
    const alloc = std.testing.allocator;
    const doc = ProfileDoc{
        .agent_id = "custom-gate",
        .stages = &[_]StageDoc{
            .{ .stage_id = "plan", .role = "echo" },
            .{ .stage_id = "implement", .role = "scout" },
            .{ .stage_id = "verify", .role = "reviewer", .skill = "clawhub://usezombie/reviewer@1.0.0", .gate = true },
        },
    };

    var profile = try fromDoc(alloc, doc);
    defer profile.deinit();
    try std.testing.expectEqualStrings("clawhub://usezombie/reviewer@1.0.0", profile.stages[2].skill_id);
}
