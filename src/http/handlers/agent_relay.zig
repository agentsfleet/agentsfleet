//! M18_003: Stateless agent relay handler.
//! Accepts CLI messages + tool definitions, forwards to workspace LLM provider,
//! streams response back as SSE events (tool_use, text_delta, done, error).
//! zombied is a pure pass-through: adds system prompt + API key, forwards, streams back.

const std = @import("std");
const httpz = @import("httpz");
const common = @import("common.zig");
const error_codes = @import("../../errors/codes.zig");
const crypto_store = @import("../../secrets/crypto_store.zig");
const nullclaw = @import("nullclaw");
const obs_log = @import("../../observability/logging.zig");

const Config = nullclaw.config.Config;
const providers = nullclaw.providers;
const ChatRequest = providers.ChatRequest;
const ChatMessage = providers.ChatMessage;
const ToolSpec = providers.ToolSpec;

const log = std.log.scoped(.http);

const SPEC_TEMPLATE_SYSTEM_PROMPT =
    \\You are a spec generation agent for the zombie platform.
    \\Explore the user's repo to understand its language, ecosystem, and structure.
    \\Use the provided tools (read_file, list_dir, glob) to examine the codebase.
    \\Generate a complete milestone spec using the canonical format.
    \\Include concrete sections, dimensions, acceptance criteria, and verification gates.
    \\Base your output on what you actually find in the repo, not assumptions.
;

const SPEC_PREVIEW_SYSTEM_PROMPT =
    \\You are a blast radius analyzer for the zombie platform.
    \\Read the spec provided, then explore the user's repo using the provided tools.
    \\Predict which files the agent will touch when implementing this spec.
    \\Output each match with confidence (high, medium, low) and a brief reason.
    \\Format: one line per file, e.g. "● src/http/router.zig  high  — new route variants needed"
;

const MAX_ROUND_TRIPS: u32 = 10;

pub const Mode = enum {
    spec_template,
    spec_preview,

    pub fn systemPrompt(self: Mode) []const u8 {
        return switch (self) {
            .spec_template => SPEC_TEMPLATE_SYSTEM_PROMPT,
            .spec_preview => SPEC_PREVIEW_SYSTEM_PROMPT,
        };
    }
};

// ── JSON request payload ────────────────────────────────────────────────────

const JsonMessage = struct {
    role: []const u8,
    content: []const u8 = "",
};

const JsonToolInputSchema = struct {
    type: []const u8 = "object",
};

const JsonTool = struct {
    name: []const u8,
    description: []const u8 = "",
    input_schema: ?JsonToolInputSchema = null,
};

const RelayRequest = struct {
    workspace_id: []const u8,
    messages: []const JsonMessage = &.{},
    tools: []const JsonTool = &.{},
};

// ── Handler entry points ────────────────────────────────────────────────────

pub fn handleSpecTemplate(
    ctx: *common.Context,
    req: *httpz.Request,
    res: *httpz.Response,
    workspace_id: []const u8,
) void {
    handleRelay(ctx, req, res, workspace_id, .spec_template);
}

pub fn handleSpecPreview(
    ctx: *common.Context,
    req: *httpz.Request,
    res: *httpz.Response,
    workspace_id: []const u8,
) void {
    handleRelay(ctx, req, res, workspace_id, .spec_preview);
}

// ── Shared relay logic ──────────────────────────────────────────────────────

fn handleRelay(
    ctx: *common.Context,
    req: *httpz.Request,
    res: *httpz.Response,
    workspace_id: []const u8,
    mode: Mode,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(res, req_id, workspace_id, "workspace_id")) return;

    const body = req.body() orelse {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    if (!common.checkBodySize(req, res, body, req_id)) return;

    const parsed = std.json.parseFromSlice(RelayRequest, alloc, body, .{
        .ignore_unknown_fields = true,
    }) catch {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();
    const rval = parsed.value;

    if (rval.messages.len == 0) {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "messages array is required and must not be empty", req_id);
        return;
    }

    // Authorize workspace
    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(res, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    // Resolve workspace LLM provider credentials
    const provider_name = crypto_store.load(alloc, conn, workspace_id, "llm_provider_preference") catch {
        common.errorResponse(res, .bad_request, error_codes.ERR_RELAY_NO_PROVIDER, "No LLM provider configured for workspace. Set credentials via PUT /v1/workspaces/{id}/credentials/llm", req_id);
        return;
    };

    const api_key_name = std.fmt.allocPrint(alloc, "{s}_api_key", .{provider_name}) catch {
        common.internalOperationError(res, "Allocation failed", req_id);
        return;
    };

    const api_key = crypto_store.load(alloc, conn, workspace_id, api_key_name) catch {
        common.errorResponse(res, .bad_request, error_codes.ERR_CRED_PLATFORM_KEY_MISSING, "LLM API key not found for workspace provider", req_id);
        return;
    };

    // Build nullclaw config with workspace credentials
    var cfg = Config.load(alloc) catch {
        common.internalOperationError(res, "Failed to load agent config", req_id);
        return;
    };
    defer cfg.deinit();
    cfg.default_provider = provider_name;

    // Inject API key into provider entry (same pattern as executor/runner.zig)
    injectProviderApiKey(&cfg, api_key) catch {
        common.internalOperationError(res, "Failed to inject provider API key", req_id);
        return;
    };

    // Initialize provider
    var runtime_provider = providers.runtime_bundle.RuntimeProviderBundle.init(alloc, &cfg) catch {
        common.internalOperationError(res, "Failed to initialize LLM provider", req_id);
        return;
    };
    defer runtime_provider.deinit();
    const provider_i = runtime_provider.provider();

    // Build ChatRequest: system prompt + CLI messages + tool specs
    const system_prompt = mode.systemPrompt();

    // Convert JSON messages to ChatMessage array
    var messages: std.ArrayList(ChatMessage) = .{};
    messages.append(alloc, ChatMessage.system(system_prompt)) catch {
        common.internalOperationError(res, "Allocation failed", req_id);
        return;
    };

    for (rval.messages) |msg| {
        const role = providers.Role.fromSlice(msg.role) orelse continue;
        messages.append(alloc, .{ .role = role, .content = msg.content }) catch continue;
    }

    // Convert JSON tools to ToolSpec array
    var tool_specs: std.ArrayList(ToolSpec) = .{};
    for (rval.tools) |t| {
        tool_specs.append(alloc, .{
            .name = t.name,
            .description = t.description,
        }) catch continue;
    }

    const model = cfg.default_model orelse "claude-sonnet-4-20250514";

    const chat_req = ChatRequest{
        .messages = messages.items,
        .model = model,
        .temperature = 0.3,
        .max_tokens = 4096,
        .tools = if (tool_specs.items.len > 0) tool_specs.items else null,
        .timeout_secs = 30,
    };

    // Call provider
    const response = provider_i.chatWithTools(alloc, chat_req) catch |err| {
        log.warn("relay.provider_error err={s} workspace_id={s}", .{ @errorName(err), workspace_id });
        // Set SSE headers and send error event
        setSseHeaders(res);
        const error_event = std.fmt.allocPrint(
            alloc,
            "event: error\ndata: {{\"message\":\"provider error: {s}\"}}\n\n",
            .{@errorName(err)},
        ) catch return;
        _ = res.chunk(error_event) catch {};
        return;
    };

    // Set SSE headers
    setSseHeaders(res);

    // Emit tool_use events if the model requested tool calls
    if (response.hasToolCalls()) {
        for (response.tool_calls) |tc| {
            const event = std.fmt.allocPrint(
                alloc,
                "event: tool_use\ndata: {{\"id\":\"{s}\",\"name\":\"{s}\",\"input\":{s}}}\n\n",
                .{ tc.id, tc.name, tc.arguments },
            ) catch continue;
            res.chunk(event) catch return;
        }
    }

    // Emit text_delta if the model returned text content
    if (response.content) |text| {
        if (text.len > 0) {
            // Escape the text for JSON embedding
            const escaped = jsonEscapeString(alloc, text) catch text;
            const event = std.fmt.allocPrint(
                alloc,
                "event: text_delta\ndata: {{\"text\":\"{s}\"}}\n\n",
                .{escaped},
            ) catch return;
            res.chunk(event) catch return;
        }
    }

    // Emit done event with usage
    const done_event = std.fmt.allocPrint(
        alloc,
        "event: done\ndata: {{\"usage\":{{\"input_tokens\":{d},\"output_tokens\":{d},\"total_tokens\":{d}}},\"provider\":\"{s}\",\"model\":\"{s}\"}}\n\n",
        .{
            response.usage.prompt_tokens,
            response.usage.completion_tokens,
            response.usage.total_tokens,
            provider_name,
            model,
        },
    ) catch return;
    _ = res.chunk(done_event) catch {};
}

/// Inject an API key into the NullClaw Config for the workspace's default provider.
/// Same pattern as executor/runner.zig:injectProviderApiKey.
fn injectProviderApiKey(cfg: *Config, api_key: []const u8) !void {
    const owned_key = try cfg.allocator.dupe(u8, api_key);

    // Try to update an existing provider entry.
    for (@constCast(cfg.providers)) |*entry| {
        if (std.mem.eql(u8, entry.name, cfg.default_provider)) {
            entry.api_key = owned_key;
            return;
        }
    }

    // No existing entry — prepend one.
    const nullclaw_config = @import("nullclaw").config;
    const new_providers = try cfg.allocator.alloc(nullclaw_config.ProviderEntry, cfg.providers.len + 1);
    new_providers[0] = .{
        .name = cfg.default_provider,
        .api_key = owned_key,
    };
    @memcpy(new_providers[1..], cfg.providers);
    cfg.providers = new_providers;
}

fn setSseHeaders(res: *httpz.Response) void {
    res.header("Content-Type", "text/event-stream");
    res.header("Cache-Control", "no-cache");
    res.header("Connection", "keep-alive");
    res.header("X-Accel-Buffering", "no");
}

/// Escape a string for safe embedding in a JSON string value.
/// Handles newlines, tabs, quotes, backslashes, and control characters.
fn jsonEscapeString(alloc: std.mem.Allocator, input: []const u8) ![]const u8 {
    var buf: std.ArrayList(u8) = .{};
    for (input) |c| {
        switch (c) {
            '"' => try buf.appendSlice(alloc, "\\\""),
            '\\' => try buf.appendSlice(alloc, "\\\\"),
            '\n' => try buf.appendSlice(alloc, "\\n"),
            '\r' => try buf.appendSlice(alloc, "\\r"),
            '\t' => try buf.appendSlice(alloc, "\\t"),
            else => {
                if (c < 0x20) {
                    const hex = std.fmt.allocPrint(alloc, "\\u{x:0>4}", .{c}) catch continue;
                    try buf.appendSlice(alloc, hex);
                } else {
                    try buf.append(alloc, c);
                }
            },
        }
    }
    return buf.toOwnedSlice(alloc);
}

// ── Tests ───────────────────────────────────────────────────────────────────

test "Mode.systemPrompt returns distinct prompts" {
    const t = Mode.spec_template.systemPrompt();
    const p = Mode.spec_preview.systemPrompt();
    try std.testing.expect(t.len > 0);
    try std.testing.expect(p.len > 0);
    try std.testing.expect(!std.mem.eql(u8, t, p));
}

test "jsonEscapeString escapes newlines and quotes" {
    const alloc = std.testing.allocator;
    const result = try jsonEscapeString(alloc, "line1\nline2\t\"quoted\"");
    defer alloc.free(result);
    try std.testing.expectEqualStrings("line1\\nline2\\t\\\"quoted\\\"", result);
}

test "jsonEscapeString handles empty input" {
    const alloc = std.testing.allocator;
    const result = try jsonEscapeString(alloc, "");
    defer alloc.free(result);
    try std.testing.expectEqualStrings("", result);
}

test "jsonEscapeString handles backslash" {
    const alloc = std.testing.allocator;
    const result = try jsonEscapeString(alloc, "a\\b");
    defer alloc.free(result);
    try std.testing.expectEqualStrings("a\\\\b", result);
}
