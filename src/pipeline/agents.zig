//! Agent skill runner and registry. Dispatches profile-loaded skill_ids to execution backends.

const std = @import("std");
const types = @import("../types.zig");
const events = @import("../events/bus.zig");
const runners = @import("agents_runner.zig");
const topology = @import("topology.zig");

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
    /// Pre-loaded prompt content for this skill. Null means use actor-dispatch
    /// against PromptFiles (default echo/scout/warden backends).
    prompt_content: ?[]const u8 = null,
};

pub const RoleBinding = struct {
    role_id: []const u8,
    skill_id: []const u8,
    actor: types.Actor,
    kind: SkillKind,
    custom_runner: ?CustomSkillFn = null,
    /// Pre-loaded prompt content for this skill. Null means use actor-dispatch
    /// against PromptFiles (default echo/scout/warden backends).
    prompt_content: ?[]const u8 = null,
};

// ── Default execution backend wrappers ───────────────────────────────────────
// These are the compiled runners for the default skills (echo/scout/warden).
// They are registered into the SkillRegistry at worker startup via
// populateRegistryFromProfile — not via a hardcoded static table.

fn echoRunner(alloc: std.mem.Allocator, _: []const u8, _: []const u8, input: RoleInput) anyerror!AgentResult {
    return runEcho(
        alloc,
        input.workspace_path,
        input.prompts.echo,
        input.spec_content orelse return RoleError.MissingRoleInput,
        input.memory_context orelse return RoleError.MissingRoleInput,
        input.execution_context orelse .{},
    );
}

fn scoutRunner(alloc: std.mem.Allocator, _: []const u8, _: []const u8, input: RoleInput) anyerror!AgentResult {
    return runScout(
        alloc,
        input.workspace_path,
        input.prompts.scout,
        input.plan_content orelse return RoleError.MissingRoleInput,
        input.defects_content,
        input.execution_context orelse .{},
    );
}

fn wardenRunner(alloc: std.mem.Allocator, _: []const u8, _: []const u8, input: RoleInput) anyerror!AgentResult {
    return runWarden(
        alloc,
        input.workspace_path,
        input.prompts.warden,
        input.spec_content orelse return RoleError.MissingRoleInput,
        input.plan_content orelse return RoleError.MissingRoleInput,
        input.implementation_summary orelse return RoleError.MissingRoleInput,
        input.execution_context orelse .{},
    );
}

// ── Default skill table (local, not exported) ─────────────────────────────────
// Maps the three default skill_ids to their compiled runners and actors.
// Used only by populateRegistryFromProfile — not a public dispatch table.
const DefaultSkillEntry = struct {
    skill_id: []const u8,
    actor: types.Actor,
    runner: CustomSkillFn,
};
const DEFAULT_SKILL_ENTRIES = [_]DefaultSkillEntry{
    .{ .skill_id = "echo", .actor = .echo, .runner = echoRunner },
    .{ .skill_id = "scout", .actor = .scout, .runner = scoutRunner },
    .{ .skill_id = "warden", .actor = .warden, .runner = wardenRunner },
};

// ── Errors ────────────────────────────────────────────────────────────────────

pub const RoleError = error{
    UnknownRole,
    MissingRoleInput,
    MissingCustomRunner,
};

pub const SkillRegistryError = error{
    InvalidSkillId,
    DuplicateSkillId,
    OutOfMemory,
};

// ── SkillRegistry ─────────────────────────────────────────────────────────────

pub const SkillRegistry = struct {
    alloc: std.mem.Allocator,
    skills: std.ArrayList(SkillBinding),

    pub fn init(alloc: std.mem.Allocator) SkillRegistry {
        return .{ .alloc = alloc, .skills = std.ArrayList(SkillBinding){} };
    }

    pub fn deinit(self: *SkillRegistry) void {
        for (self.skills.items) |skill| {
            self.alloc.free(skill.skill_id);
            if (skill.prompt_content) |pc| self.alloc.free(pc);
        }
        self.skills.deinit(self.alloc);
    }

    /// Register a skill in the registry. Dupes skill_id and prompt_content (if non-null).
    /// Returns DuplicateSkillId if the skill_id is already registered.
    pub fn registerCustomSkill(
        self: *SkillRegistry,
        skill_id: []const u8,
        actor: types.Actor,
        runner: CustomSkillFn,
        prompt_content: ?[]const u8,
    ) SkillRegistryError!void {
        if (skill_id.len == 0) return SkillRegistryError.InvalidSkillId;
        if (self.resolveSkill(skill_id) != null) return SkillRegistryError.DuplicateSkillId;

        const owned_id = try self.alloc.dupe(u8, skill_id);
        errdefer self.alloc.free(owned_id);
        const owned_prompt = if (prompt_content) |pc| try self.alloc.dupe(u8, pc) else null;
        errdefer if (owned_prompt) |pc| self.alloc.free(pc);

        try self.skills.append(self.alloc, .{
            .skill_id = owned_id,
            .actor = actor,
            .kind = .custom,
            .custom_runner = runner,
            .prompt_content = owned_prompt,
        });
    }

    pub fn resolveSkill(self: *const SkillRegistry, skill_id: []const u8) ?SkillBinding {
        for (self.skills.items) |skill| {
            if (std.ascii.eqlIgnoreCase(skill_id, skill.skill_id)) return skill;
        }
        return null;
    }
};

// ── Registry population ───────────────────────────────────────────────────────

/// Populate a SkillRegistry from a loaded profile.
/// For each stage skill_id that matches a known default backend (echo/scout/warden),
/// registers the compiled runner with null prompt_content (actor dispatch handles prompts).
/// Skill_ids with no known backend are skipped — custom runners must be registered
/// separately before this call (or via clawhub registry integration, future milestone).
pub fn populateRegistryFromProfile(
    registry: *SkillRegistry,
    profile: *const topology.Profile,
) !void {
    for (profile.stages) |stage| {
        if (registry.resolveSkill(stage.skill_id) != null) continue;
        for (DEFAULT_SKILL_ENTRIES) |e| {
            if (std.ascii.eqlIgnoreCase(stage.skill_id, e.skill_id)) {
                try registry.registerCustomSkill(stage.skill_id, e.actor, e.runner, null);
                break;
            }
        }
    }
}

// ── Resolution ────────────────────────────────────────────────────────────────

/// Resolve a role_id + skill_id pair against the registry.
/// Returns null if the skill_id is not registered — callers must handle this
/// (InvalidPipelineRole at the executor level).
pub fn resolveRoleWithRegistry(
    registry: *const SkillRegistry,
    role_id: []const u8,
    skill_id: []const u8,
) ?RoleBinding {
    const skill = registry.resolveSkill(skill_id) orelse return null;
    return .{
        .role_id = role_id,
        .skill_id = skill.skill_id,
        .actor = skill.actor,
        .kind = skill.kind,
        .custom_runner = skill.custom_runner,
        .prompt_content = skill.prompt_content,
    };
}

// ── Dispatch ─────────────────────────────────────────────────────────────────

pub fn runByRole(alloc: std.mem.Allocator, binding: RoleBinding, input: RoleInput) !AgentResult {
    return switch (binding.kind) {
        .custom => {
            const runner = binding.custom_runner orelse return RoleError.MissingCustomRunner;
            return runner(alloc, binding.role_id, binding.skill_id, input);
        },
    };
}

// ── Prompt loading ────────────────────────────────────────────────────────────

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

// ── Output parsing ────────────────────────────────────────────────────────────

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
