//! NullClaw agent runner for Echo → Scout → Warden pipeline.
//! Each agent role gets its own NullClaw Config + tool set.
//! Calls Agent.runSingle() natively — no subprocess, no stdout parsing.

const std = @import("std");
const nullclaw = @import("nullclaw");
const types = @import("../types.zig");
const events = @import("../events/bus.zig");
const log = std.log.scoped(.agents);

const Config = nullclaw.config.Config;
const Agent = nullclaw.agent.Agent;
const providers = nullclaw.providers;
const tools_mod = nullclaw.tools;
const memory_mod = nullclaw.memory;
const observability = nullclaw.observability;
const security = nullclaw.security;

// ── Result returned from each agent invocation ────────────────────────────

pub const AgentResult = struct {
    content: []const u8, // owned by caller
    token_count: u64,
    wall_seconds: u64,
    exit_ok: bool, // true if runSingle() completed without error
};

/// Emit a structured log line for a completed NullClaw agent invocation.
/// Satisfies M1_003 AC#6: NullClaw run events captured with exit code,
/// duration, and token count. Peak memory is N/A for M1.
pub fn emitNullclawRunEvent(
    run_id: []const u8,
    request_id: []const u8,
    attempt: u32,
    stage_id: []const u8,
    role_id: []const u8,
    actor: types.Actor,
    result: AgentResult,
) void {
    log.info(
        "nullclaw_run event_type=nullclaw_run run_id={s} request_id={s} attempt={d} stage_id={s} role_id={s} actor={s} exit_ok={} tokens={d} wall_seconds={d} peak_memory_kb=N/A",
        .{ run_id, request_id, attempt, stage_id, role_id, actor.label(), result.exit_ok, result.token_count, result.wall_seconds },
    );
    var detail_buf: [192]u8 = undefined;
    const detail = std.fmt.bufPrint(
        &detail_buf,
        "request_id={s} attempt={d} stage_id={s} role_id={s} actor={s} exit_ok={} tokens={d} wall_seconds={d}",
        .{ request_id, attempt, stage_id, role_id, actor.label(), result.exit_ok, result.token_count, result.wall_seconds },
    ) catch "nullclaw_run";
    events.emit("nullclaw_run", run_id, detail);
}

// ── Per-agent system prompts (loaded from config/ directory) ───────────────

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

pub const CustomSkillFn = *const fn (std.mem.Allocator, RoleBinding, RoleInput) anyerror!AgentResult;

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

pub const RoleInput = struct {
    workspace_path: []const u8,
    prompts: *const PromptFiles,
    spec_content: ?[]const u8 = null,
    memory_context: ?[]const u8 = null,
    plan_content: ?[]const u8 = null,
    defects_content: ?[]const u8 = null,
    implementation_summary: ?[]const u8 = null,
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
};

pub const SkillRegistry = struct {
    alloc: std.mem.Allocator,
    custom_skills: std.ArrayList(SkillBinding),

    pub fn init(alloc: std.mem.Allocator) SkillRegistry {
        return .{
            .alloc = alloc,
            .custom_skills = std.ArrayList(SkillBinding){},
        };
    }

    pub fn deinit(self: *SkillRegistry) void {
        for (self.custom_skills.items) |skill| {
            self.alloc.free(skill.skill_id);
        }
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
        ),
        .scout => runScout(
            alloc,
            input.workspace_path,
            input.prompts.scout,
            input.plan_content orelse return RoleError.MissingRoleInput,
            input.defects_content,
        ),
        .warden => runWarden(
            alloc,
            input.workspace_path,
            input.prompts.warden,
            input.spec_content orelse return RoleError.MissingRoleInput,
            input.plan_content orelse return RoleError.MissingRoleInput,
            input.implementation_summary orelse return RoleError.MissingRoleInput,
        ),
        .custom => {
            const runner = binding.custom_runner orelse return RoleError.MissingCustomRunner;
            return runner(alloc, binding, input);
        },
    };
}

pub fn loadPrompts(alloc: std.mem.Allocator, config_dir: []const u8) !PromptFiles {
    return PromptFiles{
        .echo = try readFile(alloc, config_dir, "echo-prompt.md"),
        .scout = try readFile(alloc, config_dir, "scout-prompt.md"),
        .warden = try readFile(alloc, config_dir, "warden-prompt.md"),
    };
}

fn readFile(alloc: std.mem.Allocator, dir: []const u8, name: []const u8) ![]const u8 {
    const path = try std.fmt.allocPrint(alloc, "{s}/{s}", .{ dir, name });
    defer alloc.free(path);
    const file = try std.fs.openFileAbsolute(path, .{});
    defer file.close();
    return file.readToEndAlloc(alloc, 512 * 1024);
}

// ── Base config init ──────────────────────────────────────────────────────

/// Build a NullClaw Config for a given agent role.
/// Reads the base config from ~/.nullclaw/config.json (which has the LLM API key)
/// then overrides workspace_dir and autonomy settings per-role.
fn buildConfig(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
) !Config {
    var cfg = try Config.load(alloc);
    cfg.workspace_dir = workspace_path;
    return cfg;
}

const ObserverBackend = enum {
    log,
    noop,
    verbose,
};

const ObserverRuntime = struct {
    backend: ObserverBackend,
    noop: observability.NoopObserver = .{},
    log_observer: observability.LogObserver = .{},
    verbose_observer: observability.VerboseObserver = .{},

    fn init(alloc: std.mem.Allocator) ObserverRuntime {
        return .{
            .backend = configuredObserverBackend(alloc),
        };
    }

    fn observer(self: *ObserverRuntime) observability.Observer {
        return switch (self.backend) {
            .log => self.log_observer.observer(),
            .noop => self.noop.observer(),
            .verbose => self.verbose_observer.observer(),
        };
    }
};

fn configuredObserverBackend(alloc: std.mem.Allocator) ObserverBackend {
    const raw = std.process.getEnvVarOwned(alloc, "NULLCLAW_OBSERVER") catch return .log;
    defer alloc.free(raw);
    return parseObserverBackend(raw) orelse .log;
}

fn parseObserverBackend(raw: []const u8) ?ObserverBackend {
    if (std.ascii.eqlIgnoreCase(raw, "log")) return .log;
    if (std.ascii.eqlIgnoreCase(raw, "noop")) return .noop;
    if (std.ascii.eqlIgnoreCase(raw, "verbose")) return .verbose;
    return null;
}

// ── Echo — The Planner ────────────────────────────────────────────────────
// Tools: file_read only (read-only mode, no shell, no writes)

pub fn runEcho(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    system_prompt: []const u8,
    spec_content: []const u8,
    memory_context: []const u8,
) !AgentResult {
    log.info("echo start workspace={s}", .{workspace_path});
    const start = std.time.milliTimestamp();

    var cfg = try buildConfig(alloc, workspace_path);
    defer cfg.deinit();

    // Echo: read-only — file_read + memory tools only
    var runtime_provider = try providers.runtime_bundle.RuntimeProviderBundle.init(alloc, &cfg);
    defer runtime_provider.deinit();
    const provider_i = runtime_provider.provider();

    const tools = try buildEchoTools(alloc, workspace_path, &cfg);
    defer tools_mod.deinitTools(alloc, tools);

    var mem_rt = memory_mod.initRuntime(alloc, &cfg.memory, workspace_path);
    defer if (mem_rt) |*rt| rt.deinit();
    const mem_opt: ?memory_mod.Memory = if (mem_rt) |rt| rt.memory else null;
    tools_mod.bindMemoryTools(tools, mem_opt);

    var obs_runtime = ObserverRuntime.init(alloc);
    const obs = obs_runtime.observer();

    var agent = try Agent.fromConfig(alloc, &cfg, provider_i, tools, mem_opt, obs);
    defer agent.deinit();

    // Build the prompt: system prompt + spec + memory context
    const message = try std.fmt.allocPrint(alloc,
        \\{s}
        \\
        \\---
        \\## Spec content
        \\
        \\{s}
        \\
        \\{s}
        \\
        \\Produce a plan.json file in the workspace at docs/runs/<run_id>/plan.json.
    , .{ system_prompt, spec_content, memory_context });
    defer alloc.free(message);

    const response = try agent.runSingle(message);
    // runSingle returns a borrowed slice — dupe to own it
    const owned = try alloc.dupe(u8, response);

    const elapsed_ms = std.time.milliTimestamp() - start;
    log.info("echo done tokens={d} ms={d}", .{ agent.tokensUsed(), elapsed_ms });

    return AgentResult{
        .content = owned,
        .token_count = agent.tokensUsed(),
        .wall_seconds = @as(u64, @intCast(@max(0, elapsed_ms))) / 1000,
        .exit_ok = true,
    };
}

// ── Scout — The Builder ───────────────────────────────────────────────────
// Tools: file_read, file_write, file_edit, shell, git, memory

pub fn runScout(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    system_prompt: []const u8,
    plan_content: []const u8,
    defects_content: ?[]const u8,
) !AgentResult {
    log.info("scout start workspace={s}", .{workspace_path});
    const start = std.time.milliTimestamp();

    var cfg = try buildConfig(alloc, workspace_path);
    defer cfg.deinit();

    var runtime_provider = try providers.runtime_bundle.RuntimeProviderBundle.init(alloc, &cfg);
    defer runtime_provider.deinit();
    const provider_i = runtime_provider.provider();

    // Scout: full tools
    const tools = try tools_mod.allTools(alloc, workspace_path, .{
        .allowed_paths = &.{workspace_path},
        .tools_config = cfg.tools,
    });
    defer tools_mod.deinitTools(alloc, tools);

    var mem_rt = memory_mod.initRuntime(alloc, &cfg.memory, workspace_path);
    defer if (mem_rt) |*rt| rt.deinit();
    const mem_opt: ?memory_mod.Memory = if (mem_rt) |rt| rt.memory else null;
    tools_mod.bindMemoryTools(tools, mem_opt);

    var obs_runtime = ObserverRuntime.init(alloc);
    const obs = obs_runtime.observer();

    var agent = try Agent.fromConfig(alloc, &cfg, provider_i, tools, mem_opt, obs);
    defer agent.deinit();

    const defects_section = if (defects_content) |d|
        try std.fmt.allocPrint(alloc, "\n\n## Defects from previous attempt\n\n{s}", .{d})
    else
        try alloc.dupe(u8, "");
    defer alloc.free(defects_section);

    const message = try std.fmt.allocPrint(alloc,
        \\{s}
        \\
        \\---
        \\## Plan
        \\
        \\{s}
        \\{s}
        \\
        \\Implement the plan. Write code. Produce implementation.md summarising what was done.
    , .{ system_prompt, plan_content, defects_section });
    defer alloc.free(message);

    const response = try agent.runSingle(message);
    const owned = try alloc.dupe(u8, response);

    const elapsed_ms = std.time.milliTimestamp() - start;
    log.info("scout done tokens={d} ms={d}", .{ agent.tokensUsed(), elapsed_ms });

    return AgentResult{
        .content = owned,
        .token_count = agent.tokensUsed(),
        .wall_seconds = @as(u64, @intCast(@max(0, elapsed_ms))) / 1000,
        .exit_ok = true,
    };
}

// ── Warden — The Validator ────────────────────────────────────────────────
// Tools: file_read, shell (for tests), memory. No file_write, file_edit.

pub fn runWarden(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    system_prompt: []const u8,
    spec_content: []const u8,
    plan_content: []const u8,
    implementation_summary: []const u8,
) !AgentResult {
    log.info("warden start workspace={s}", .{workspace_path});
    const start = std.time.milliTimestamp();

    var cfg = try buildConfig(alloc, workspace_path);
    defer cfg.deinit();

    var runtime_provider = try providers.runtime_bundle.RuntimeProviderBundle.init(alloc, &cfg);
    defer runtime_provider.deinit();
    const provider_i = runtime_provider.provider();

    // Warden: file_read + shell only (no write tools)
    const tools = try buildWardenTools(alloc, workspace_path, &cfg);
    defer tools_mod.deinitTools(alloc, tools);

    var mem_rt = memory_mod.initRuntime(alloc, &cfg.memory, workspace_path);
    defer if (mem_rt) |*rt| rt.deinit();
    const mem_opt: ?memory_mod.Memory = if (mem_rt) |rt| rt.memory else null;
    tools_mod.bindMemoryTools(tools, mem_opt);

    var obs_runtime = ObserverRuntime.init(alloc);
    const obs = obs_runtime.observer();

    var agent = try Agent.fromConfig(alloc, &cfg, provider_i, tools, mem_opt, obs);
    defer agent.deinit();

    const message = try std.fmt.allocPrint(alloc,
        \\{s}
        \\
        \\---
        \\## Original spec
        \\
        \\{s}
        \\
        \\## Plan that was implemented
        \\
        \\{s}
        \\
        \\## Implementation summary
        \\
        \\{s}
        \\
        \\Review the implementation against the spec. Run tests if present.
        \\Produce validation.md with tiered findings (T1=critical, T2=significant,
        \\T3=minor, T4=suggestion). A PASS verdict requires no T1 or T2 findings.
        \\Extract workspace observations (patterns, pitfalls, learnings) as bullet points
        \\under a "## Workspace observations" section for cross-run memory.
    , .{ system_prompt, spec_content, plan_content, implementation_summary });
    defer alloc.free(message);

    const response = try agent.runSingle(message);
    const owned = try alloc.dupe(u8, response);

    const elapsed_ms = std.time.milliTimestamp() - start;
    log.info("warden done tokens={d} ms={d}", .{ agent.tokensUsed(), elapsed_ms });

    return AgentResult{
        .content = owned,
        .token_count = agent.tokensUsed(),
        .wall_seconds = @as(u64, @intCast(@max(0, elapsed_ms))) / 1000,
        .exit_ok = true,
    };
}

// ── Tool builders per agent role ──────────────────────────────────────────

/// Echo tools: file_read + memory (no shell, no write)
fn buildEchoTools(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    cfg: *const Config,
) ![]tools_mod.Tool {
    return buildRestrictedTools(alloc, workspace_path, cfg, .{
        .include_shell = false,
        .include_memory_list = true,
    });
}

const RestrictedToolOptions = struct {
    include_shell: bool = false,
    include_memory_list: bool = false,
};

fn buildRestrictedTools(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    cfg: *const Config,
    opts: RestrictedToolOptions,
) ![]tools_mod.Tool {
    var list: std.ArrayList(tools_mod.Tool) = .{};
    errdefer {
        for (list.items) |t| t.deinit(alloc);
        list.deinit(alloc);
    }

    if (opts.include_shell) {
        try appendTool(alloc, &list, tools_mod.shell.ShellTool{
            .workspace_dir = workspace_path,
            .allowed_paths = cfg.autonomy.allowed_paths,
            .timeout_ns = cfg.tools.shell_timeout_secs * std.time.ns_per_s,
            .max_output_bytes = cfg.tools.shell_max_output_bytes,
        });
    }

    try appendTool(alloc, &list, tools_mod.file_read.FileReadTool{
        .workspace_dir = workspace_path,
        .allowed_paths = cfg.autonomy.allowed_paths,
        .max_file_size = cfg.tools.max_file_size_bytes,
    });

    try appendTool(alloc, &list, tools_mod.memory_recall.MemoryRecallTool{});

    if (opts.include_memory_list) {
        // Echo needs memory list for broad recall context.
        try appendTool(alloc, &list, tools_mod.memory_list.MemoryListTool{});
    }

    return list.toOwnedSlice(alloc);
}

fn appendTool(
    alloc: std.mem.Allocator,
    list: *std.ArrayList(tools_mod.Tool),
    tool_value: anytype,
) !void {
    const ToolType = @TypeOf(tool_value);
    const ptr = try alloc.create(ToolType);
    ptr.* = tool_value;
    try list.append(alloc, ptr.tool());
}

/// Warden tools: file_read + shell + memory recall (no write/edit)
fn buildWardenTools(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    cfg: *const Config,
) ![]tools_mod.Tool {
    return buildRestrictedTools(alloc, workspace_path, cfg, .{
        .include_shell = true,
        .include_memory_list = false,
    });
}

/// Parse Warden's verdict: PASS if no T1/T2 findings, FAIL otherwise.
pub fn parseWardenVerdict(content: []const u8) bool {
    // Simple heuristic: look for explicit PASS/FAIL verdict line,
    // or check for T1/T2 findings sections
    if (std.mem.containsAtLeast(u8, content, 1, "verdict: PASS") or
        std.mem.containsAtLeast(u8, content, 1, "**PASS**") or
        std.mem.containsAtLeast(u8, content, 1, "Verdict: PASS"))
    {
        // Double-check: if T1/T2 section exists with content, it's a FAIL
        if (std.mem.containsAtLeast(u8, content, 1, "### T1") or
            std.mem.containsAtLeast(u8, content, 1, "### T2"))
        {
            return false;
        }
        return true;
    }
    return false;
}

/// Extract workspace observations section from Warden's output.
/// Caller owns the result.
pub fn extractObservations(alloc: std.mem.Allocator, content: []const u8) ![]const u8 {
    const marker = "## Workspace observations";
    const start = std.mem.indexOf(u8, content, marker) orelse return try alloc.dupe(u8, "");
    const section = content[start + marker.len ..];
    // Take until the next ## heading or end of file
    const end = std.mem.indexOf(u8, section, "\n## ") orelse section.len;
    return alloc.dupe(u8, std.mem.trim(u8, section[0..end], " \t\r\n"));
}

test "parseObserverBackend supports known values" {
    try std.testing.expectEqual(@as(?ObserverBackend, .log), parseObserverBackend("log"));
    try std.testing.expectEqual(@as(?ObserverBackend, .log), parseObserverBackend("LOG"));
    try std.testing.expectEqual(@as(?ObserverBackend, .noop), parseObserverBackend("noop"));
    try std.testing.expectEqual(@as(?ObserverBackend, .verbose), parseObserverBackend("verbose"));
    try std.testing.expectEqual(@as(?ObserverBackend, null), parseObserverBackend("otel"));
}

test "lookupRole resolves built-in role ids" {
    const echo = lookupRole("echo") orelse return error.TestExpectedRole;
    try std.testing.expectEqual(types.Actor.echo, echo.actor);
    try std.testing.expectEqual(SkillKind.echo, echo.kind);

    const scout = lookupRole("SCOUT") orelse return error.TestExpectedRole;
    try std.testing.expectEqual(types.Actor.scout, scout.actor);
    try std.testing.expectEqual(SkillKind.scout, scout.kind);

    try std.testing.expectEqual(@as(?RoleBinding, null), lookupRole("security"));
}

fn testCustomSkill(
    alloc: std.mem.Allocator,
    binding: RoleBinding,
    input: RoleInput,
) !AgentResult {
    _ = alloc;
    _ = input;
    return .{
        .content = binding.role_id,
        .token_count = 1,
        .wall_seconds = 0,
        .exit_ok = true,
    };
}

test "custom skill registration and dispatch works for non built-in role" {
    var registry = SkillRegistry.init(std.testing.allocator);
    defer registry.deinit();

    try registry.registerCustomSkill("security-reviewer", .orchestrator, testCustomSkill);
    const binding = resolveRoleWithRegistry(&registry, "security", "security-reviewer") orelse return error.TestExpectedRole;
    try std.testing.expectEqual(SkillKind.custom, binding.kind);
    try std.testing.expectEqual(types.Actor.orchestrator, binding.actor);

    const fake_prompts = PromptFiles{
        .echo = "echo prompt",
        .scout = "scout prompt",
        .warden = "warden prompt",
    };
    const result = try runByRole(std.testing.allocator, binding, .{
        .workspace_path = "/tmp",
        .prompts = &fake_prompts,
    });
    try std.testing.expectEqualStrings("security", result.content);
}

test "runByRole validates required stage input fields" {
    const binding = lookupRole("warden") orelse return error.TestExpectedRole;
    const fake_prompts = PromptFiles{
        .echo = "echo prompt",
        .scout = "scout prompt",
        .warden = "warden prompt",
    };

    try std.testing.expectError(RoleError.MissingRoleInput, runByRole(std.testing.allocator, binding, .{
        .workspace_path = "/tmp",
        .prompts = &fake_prompts,
        .spec_content = "spec",
        .plan_content = "plan",
    }));
}
