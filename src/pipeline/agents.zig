//! NullClaw agent runner for Echo → Scout → Warden pipeline.

const std = @import("std");
const types = @import("../types.zig");
const events = @import("../events/bus.zig");
const runners = @import("agents_runner.zig");

const log = std.log.scoped(.agents);

pub const AgentResult = runners.AgentResult;
pub const ObserverBackend = runners.ObserverBackend;
pub const parseObserverBackend = runners.parseObserverBackend;
pub const runEcho = runners.runEcho;
pub const runScout = runners.runScout;
pub const runWarden = runners.runWarden;

pub fn emitNullclawRunEvent(
    run_id: []const u8,
    request_id: []const u8,
    trace_id: []const u8,
    attempt: u32,
    stage_id: []const u8,
    role_id: []const u8,
    actor: types.Actor,
    result: AgentResult,
) void {
    log.info(
        "nullclaw_run event_type=nullclaw_run run_id={s} request_id={s} trace_id={s} attempt={d} stage_id={s} role_id={s} actor={s} exit_ok={} tokens={d} wall_seconds={d} peak_memory_kb=N/A",
        .{ run_id, request_id, trace_id, attempt, stage_id, role_id, actor.label(), result.exit_ok, result.token_count, result.wall_seconds },
    );
    var detail_buf: [256]u8 = undefined;
    const detail = std.fmt.bufPrint(
        &detail_buf,
        "request_id={s} trace_id={s} attempt={d} stage_id={s} role_id={s} actor={s} exit_ok={} tokens={d} wall_seconds={d}",
        .{ request_id, trace_id, attempt, stage_id, role_id, actor.label(), result.exit_ok, result.token_count, result.wall_seconds },
    ) catch "nullclaw_run";
    events.emit("nullclaw_run", run_id, detail);
}

pub const PromptFiles = struct {
    echo: []const u8,
    scout: []const u8,
    warden: []const u8,
};

pub const SkillKind = enum {
    echo,
    scout,
    warden,
    custom,
};

pub const RoleInput = struct {
    workspace_path: []const u8,
    prompts: *const PromptFiles,
    spec_content: ?[]const u8 = null,
    memory_context: ?[]const u8 = null,
    plan_content: ?[]const u8 = null,
    defects_content: ?[]const u8 = null,
    implementation_summary: ?[]const u8 = null,
    execution_context: ?runners.ExecutionContext = null,
};

pub const CustomSkillFn = *const fn (std.mem.Allocator, []const u8, []const u8, RoleInput) anyerror!AgentResult;

pub const SkillBinding = struct {
    skill_id: []const u8,
    actor: types.Actor,
    kind: SkillKind,
    custom_runner: ?CustomSkillFn = null,
};

pub const RoleBinding = struct {
    role_id: []const u8,
    skill_id: []const u8,
    actor: types.Actor,
    kind: SkillKind,
    custom_runner: ?CustomSkillFn = null,
};

const BUILTIN_SKILLS = [_]SkillBinding{
    .{ .skill_id = "echo", .actor = .echo, .kind = .echo },
    .{ .skill_id = "scout", .actor = .scout, .kind = .scout },
    .{ .skill_id = "warden", .actor = .warden, .kind = .warden },
};

pub const RoleError = error{
    UnknownRole,
    MissingRoleInput,
    MissingCustomRunner,
};

pub fn lookupRole(role_id: []const u8) ?RoleBinding {
    return resolveRole(role_id, role_id);
}

pub fn resolveRole(role_id: []const u8, skill_id: []const u8) ?RoleBinding {
    for (BUILTIN_SKILLS) |skill| {
        if (!std.ascii.eqlIgnoreCase(skill_id, skill.skill_id)) continue;
        return .{
            .role_id = role_id,
            .skill_id = skill.skill_id,
            .actor = skill.actor,
            .kind = skill.kind,
            .custom_runner = null,
        };
    }
    return null;
}

pub const SkillRegistryError = error{
    InvalidSkillId,
    DuplicateSkillId,
    OutOfMemory,
};

pub const SkillRegistry = struct {
    alloc: std.mem.Allocator,
    custom_skills: std.ArrayList(SkillBinding),

    pub fn init(alloc: std.mem.Allocator) SkillRegistry {
        return .{ .alloc = alloc, .custom_skills = std.ArrayList(SkillBinding){} };
    }

    pub fn deinit(self: *SkillRegistry) void {
        for (self.custom_skills.items) |skill| self.alloc.free(skill.skill_id);
        self.custom_skills.deinit(self.alloc);
    }

    pub fn registerCustomSkill(
        self: *SkillRegistry,
        skill_id: []const u8,
        actor: types.Actor,
        runner: CustomSkillFn,
    ) SkillRegistryError!void {
        if (skill_id.len == 0) return SkillRegistryError.InvalidSkillId;
        if (resolveBuiltInSkill(skill_id) != null) return SkillRegistryError.DuplicateSkillId;
        if (self.resolveSkill(skill_id) != null) return SkillRegistryError.DuplicateSkillId;

        try self.custom_skills.append(self.alloc, .{
            .skill_id = try self.alloc.dupe(u8, skill_id),
            .actor = actor,
            .kind = .custom,
            .custom_runner = runner,
        });
    }

    pub fn resolveSkill(self: *const SkillRegistry, skill_id: []const u8) ?SkillBinding {
        for (self.custom_skills.items) |skill| {
            if (std.ascii.eqlIgnoreCase(skill_id, skill.skill_id)) return skill;
        }
        return null;
    }
};

pub fn resolveRoleWithRegistry(registry: *const SkillRegistry, role_id: []const u8, skill_id: []const u8) ?RoleBinding {
    if (resolveRole(role_id, skill_id)) |built_in| return built_in;
    const skill = registry.resolveSkill(skill_id) orelse return null;
    return .{
        .role_id = role_id,
        .skill_id = skill.skill_id,
        .actor = skill.actor,
        .kind = skill.kind,
        .custom_runner = skill.custom_runner,
    };
}

fn resolveBuiltInSkill(skill_id: []const u8) ?SkillBinding {
    for (BUILTIN_SKILLS) |skill| {
        if (std.ascii.eqlIgnoreCase(skill_id, skill.skill_id)) return skill;
    }
    return null;
}

pub fn runByRole(alloc: std.mem.Allocator, binding: RoleBinding, input: RoleInput) !AgentResult {
    return switch (binding.kind) {
        .echo => runEcho(
            alloc,
            input.workspace_path,
            input.prompts.echo,
            input.spec_content orelse return RoleError.MissingRoleInput,
            input.memory_context orelse return RoleError.MissingRoleInput,
            input.execution_context orelse .{},
        ),
        .scout => runScout(
            alloc,
            input.workspace_path,
            input.prompts.scout,
            input.plan_content orelse return RoleError.MissingRoleInput,
            input.defects_content,
            input.execution_context orelse .{},
        ),
        .warden => runWarden(
            alloc,
            input.workspace_path,
            input.prompts.warden,
            input.spec_content orelse return RoleError.MissingRoleInput,
            input.plan_content orelse return RoleError.MissingRoleInput,
            input.implementation_summary orelse return RoleError.MissingRoleInput,
            input.execution_context orelse .{},
        ),
        .custom => {
            const runner = binding.custom_runner orelse return RoleError.MissingCustomRunner;
            return runner(alloc, binding.role_id, binding.skill_id, input);
        },
    };
}

pub fn loadPrompts(alloc: std.mem.Allocator, config_dir: []const u8) !PromptFiles {
    const echo = try readFile(alloc, config_dir, "echo-prompt.md");
    errdefer alloc.free(echo);
    const scout = try readFile(alloc, config_dir, "scout-prompt.md");
    errdefer alloc.free(scout);
    const warden = try readFile(alloc, config_dir, "warden-prompt.md");
    return .{ .echo = echo, .scout = scout, .warden = warden };
}

fn readFile(alloc: std.mem.Allocator, dir: []const u8, name: []const u8) ![]const u8 {
    const path = try std.fmt.allocPrint(alloc, "{s}/{s}", .{ dir, name });
    defer alloc.free(path);
    const file = try std.fs.cwd().openFile(path, .{});
    defer file.close();
    return file.readToEndAlloc(alloc, 512 * 1024);
}

pub fn parseWardenVerdict(content: []const u8) bool {
    if (std.mem.containsAtLeast(u8, content, 1, "verdict: PASS") or
        std.mem.containsAtLeast(u8, content, 1, "**PASS**") or
        std.mem.containsAtLeast(u8, content, 1, "Verdict: PASS"))
    {
        if (std.mem.containsAtLeast(u8, content, 1, "### T1") or
            std.mem.containsAtLeast(u8, content, 1, "### T2"))
        {
            return false;
        }
        return true;
    }
    return false;
}

pub fn extractObservations(alloc: std.mem.Allocator, content: []const u8) ![]const u8 {
    const marker = "## Workspace observations";
    const start = std.mem.indexOf(u8, content, marker) orelse return try alloc.dupe(u8, "");
    const section = content[start + marker.len ..];
    const end = std.mem.indexOf(u8, section, "\n## ") orelse section.len;
    return alloc.dupe(u8, std.mem.trim(u8, section[0..end], " \t\r\n"));
}

test {
    _ = @import("agents_test.zig");
}
