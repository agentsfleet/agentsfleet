//! Builder functions for each NullClaw built-in tool.
//!
//! Each function takes a BuildCtx and returns a NullClaw Tool.
//! Called by tool_bridge.zig when a zombie spec explicitly lists tools.
//!
//! Binary boundary: imports only `nullclaw`. Must NOT import from
//! src/zombie/, src/pipeline/, or src/main.zig.

const std = @import("std");
const nullclaw = @import("nullclaw");
const tools_mod = nullclaw.tools;
const Config = nullclaw.config.Config;
const bridge = @import("tool_bridge.zig");
const BuildCtx = bridge.BuildCtx;
const PolicyHttpRequestTool = @import("runtime/policy_http_request.zig");

// ── Core file tools ────────────────────────────────────────────────────────

pub fn buildShell(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const tc = ctx.cfg.tools;
    const ptr = try ctx.alloc.create(tools_mod.shell.ShellTool);
    ptr.* = .{
        .workspace_dir = ctx.workspace_path,
        .allowed_paths = ctx.cfg.autonomy.allowed_paths,
        .timeout_ns = tc.shell_timeout_secs * std.time.ns_per_s,
        .max_output_bytes = tc.shell_max_output_bytes,
        .policy = null,
        .path_env_vars = tc.path_env_vars,
    };
    // Sandbox setup requires NullClaw-internal createSandbox() which is not
    // pub-exported. The executor workspace is already isolated (temporary
    // worktree, deleted after run), so sandbox=null is safe here.
    return ptr.tool();
}

pub fn buildFileRead(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.file_read.FileReadTool);
    ptr.* = .{
        .workspace_dir = ctx.workspace_path,
        .allowed_paths = ctx.cfg.autonomy.allowed_paths,
        .max_file_size = ctx.cfg.tools.max_file_size_bytes,
    };
    return ptr.tool();
}

pub fn buildFileWrite(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.file_write.FileWriteTool);
    ptr.* = .{
        .workspace_dir = ctx.workspace_path,
        .allowed_paths = ctx.cfg.autonomy.allowed_paths,
    };
    return ptr.tool();
}

pub fn buildFileEdit(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.file_edit.FileEditTool);
    ptr.* = .{
        .workspace_dir = ctx.workspace_path,
        .allowed_paths = ctx.cfg.autonomy.allowed_paths,
        .max_file_size = ctx.cfg.tools.max_file_size_bytes,
    };
    return ptr.tool();
}

pub fn buildFileAppend(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.file_append.FileAppendTool);
    ptr.* = .{
        .workspace_dir = ctx.workspace_path,
        .allowed_paths = ctx.cfg.autonomy.allowed_paths,
        .max_file_size = ctx.cfg.tools.max_file_size_bytes,
    };
    return ptr.tool();
}

pub fn buildFileDelete(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.file_delete.FileDeleteTool);
    ptr.* = .{
        .workspace_dir = ctx.workspace_path,
        .allowed_paths = ctx.cfg.autonomy.allowed_paths,
    };
    return ptr.tool();
}

pub fn buildFileReadHashed(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.file_read_hashed.FileReadHashedTool);
    ptr.* = .{
        .workspace_dir = ctx.workspace_path,
        .allowed_paths = ctx.cfg.autonomy.allowed_paths,
        .max_file_size = ctx.cfg.tools.max_file_size_bytes,
    };
    return ptr.tool();
}

pub fn buildFileEditHashed(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.file_edit_hashed.FileEditHashedTool);
    ptr.* = .{
        .workspace_dir = ctx.workspace_path,
        .allowed_paths = ctx.cfg.autonomy.allowed_paths,
        .max_file_size = ctx.cfg.tools.max_file_size_bytes,
    };
    return ptr.tool();
}

// ── Git ────────────────────────────────────────────────────────────────────

pub fn buildGit(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.git.GitTool);
    ptr.* = .{ .workspace_dir = ctx.workspace_path };
    return ptr.tool();
}

// ── Stateless tools ────────────────────────────────────────────────────────

pub fn buildImage(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.image.ImageInfoTool);
    ptr.* = .{};
    return ptr.tool();
}

pub fn buildCalculator(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.calculator.CalculatorTool);
    ptr.* = .{};
    return ptr.tool();
}

// ── Memory tools ───────────────────────────────────────────────────────────

pub fn buildMemoryStore(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.memory_store.MemoryStoreTool);
    ptr.* = .{};
    return ptr.tool();
}

pub fn buildMemoryRecall(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.memory_recall.MemoryRecallTool);
    ptr.* = .{};
    return ptr.tool();
}

pub fn buildMemoryList(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.memory_list.MemoryListTool);
    ptr.* = .{};
    return ptr.tool();
}

pub fn buildMemoryForget(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.memory_forget.MemoryForgetTool);
    ptr.* = .{};
    return ptr.tool();
}

// ── Agent orchestration ────────────────────────────────────────────────────

pub fn buildDelegate(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.delegate.DelegateTool);
    ptr.* = .{
        .agents = &.{},
        .configured_providers = &.{},
        .fallback_api_key = null,
        .depth = 0,
    };
    return ptr.tool();
}

pub fn buildSchedule(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.schedule.ScheduleTool);
    ptr.* = .{};
    return ptr.tool();
}

pub fn buildSpawn(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.spawn.SpawnTool);
    ptr.* = .{ .manager = null };
    return ptr.tool();
}

// ── Network tools (HTTP/search/fetch) ──────────────────────────────────────

pub fn buildHttpRequest(ctx: BuildCtx) anyerror!tools_mod.Tool {
    // When the session carries an ExecutionPolicy, the policy-aware variant
    // owns substitution + per-execution allowlist on the same boundary.
    // Without a policy we fall back to plain NullClaw — relevant for the
    // executor unit-test path that drives the bridge with no session.
    if (ctx.policy) |policy_ptr| {
        const ptr = try ctx.alloc.create(PolicyHttpRequestTool);
        // `inner.allowed_domains` mirrors the per-execution allowlist so
        // NullClaw treats these hosts as operator-trusted: skip SSRF, allow
        // private-IP resolution for hosts the zombie config explicitly
        // declared. Our outer allowlist remains authoritative — anything
        // off-list never reaches the inner tool.
        ptr.* = .{
            .policy = policy_ptr,
            .inner = .{
                .allowed_domains = policy_ptr.network_policy.allow,
                .max_response_size = 1_000_000,
                .timeout_secs = ctx.cfg.tools.shell_timeout_secs,
            },
        };
        return ptr.tool();
    }
    const ptr = try ctx.alloc.create(tools_mod.http_request.HttpRequestTool);
    ptr.* = .{
        .allowed_domains = &.{},
        .max_response_size = 1_000_000,
        .timeout_secs = ctx.cfg.tools.shell_timeout_secs,
    };
    return ptr.tool();
}

pub fn buildWebSearch(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.web_search.WebSearchTool);
    ptr.* = .{
        .searxng_base_url = null,
        .provider = "auto",
        .fallback_providers = &.{},
        .timeout_secs = ctx.cfg.tools.shell_timeout_secs,
    };
    return ptr.tool();
}

pub fn buildWebFetch(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.web_fetch.WebFetchTool);
    ptr.* = .{
        .default_max_chars = ctx.cfg.tools.web_fetch_max_chars,
        .allowed_domains = &.{},
    };
    return ptr.tool();
}

pub fn buildPushover(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.pushover.PushoverTool);
    ptr.* = .{ .workspace_dir = ctx.workspace_path };
    return ptr.tool();
}

// ── Browser tools ──────────────────────────────────────────────────────────

pub fn buildBrowser(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.browser.BrowserTool);
    ptr.* = .{};
    return ptr.tool();
}

pub fn buildScreenshot(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.screenshot.ScreenshotTool);
    ptr.* = .{ .workspace_dir = ctx.workspace_path };
    return ptr.tool();
}

pub fn buildBrowserOpen(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.browser_open.BrowserOpenTool);
    ptr.* = .{ .allowed_domains = &.{} };
    return ptr.tool();
}

// ── Cron tools ─────────────────────────────────────────────────────────────

pub fn buildCronAdd(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.cron_add.CronAddTool);
    ptr.* = .{};
    return ptr.tool();
}

pub fn buildCronList(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.cron_list.CronListTool);
    ptr.* = .{};
    return ptr.tool();
}

pub fn buildCronRemove(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.cron_remove.CronRemoveTool);
    ptr.* = .{};
    return ptr.tool();
}

pub fn buildCronRun(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.cron_run.CronRunTool);
    ptr.* = .{};
    return ptr.tool();
}

pub fn buildCronRuns(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.cron_runs.CronRunsTool);
    ptr.* = .{};
    return ptr.tool();
}

pub fn buildCronUpdate(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.cron_update.CronUpdateTool);
    ptr.* = .{};
    return ptr.tool();
}

// ── Misc tools ─────────────────────────────────────────────────────────────

pub fn buildMessage(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.message.MessageTool);
    ptr.* = .{};
    return ptr.tool();
}
