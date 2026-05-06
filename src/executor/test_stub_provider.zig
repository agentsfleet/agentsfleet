//! Test-only NullClaw provider that emits a canned tool_use response.
//!
//! Compiled into `zombied-executor-stub` only — production and harness
//! binaries strip this module via the `executor_provider_stub` build option
//! comptime branch in stub_provider_gate.zig.
//!
//! On the first `chat()` call, returns a ChatResponse whose `content` and
//! whose tool_call `arguments` both contain `SYNTHETIC_SECRET` literally.
//! On subsequent calls, returns an empty assistant response so the agent
//! loop terminates after the tool round-trip. `supports_streaming` is true
//! so the agent drives the streaming path; with no `stream_chat` slot
//! NullClaw falls back to chat() + emitChatResponseAsStream, which emits
//! `content` as a textDelta chunk (exercising the chunk-redaction path)
//! and then notifies tool_use observers (exercising the tool-arg path).
//!
//! The redactor (`runner_progress.Adapter`) replaces every byte of the
//! resolved secret on every outgoing frame with the placeholder string
//! `${secrets.llm.api_key}` — the test fixture wires SYNTHETIC_SECRET
//! into agent_config.api_key so collectSecrets sees the same bytes.

const std = @import("std");
const nullclaw = @import("nullclaw");

const providers = nullclaw.providers;
const Provider = providers.Provider;
const ChatRequest = providers.ChatRequest;
const ChatResponse = providers.ChatResponse;
const ToolCall = providers.ToolCall;
const TokenUsage = providers.TokenUsage;

pub const SYNTHETIC_SECRET = "ZMBSTUB-redaction-canary-9c8f4e1a2d";

const CANNED_TOOL_NAME = "stub_canary_tool";
const CANNED_TOOL_CALL_ID = "call_stub_001";
const CONTENT_TEMPLATE = "Calling tool with token=" ++ SYNTHETIC_SECRET;
const ARGS_TEMPLATE = "{\"token\":\"" ++ SYNTHETIC_SECRET ++ "\"}";

pub const StubProvider = struct {
    allocator: std.mem.Allocator,
    call_count: u32 = 0,

    pub fn init(allocator: std.mem.Allocator) StubProvider {
        return .{ .allocator = allocator };
    }

    pub fn provider(self: *StubProvider) Provider {
        return .{ .ptr = self, .vtable = &vtable };
    }

    const vtable: Provider.VTable = .{
        .chatWithSystem = chatWithSystemImpl,
        .chat = chatImpl,
        .supportsNativeTools = supportsNativeToolsImpl,
        .getName = getNameImpl,
        .deinit = deinitImpl,
        .supports_streaming = supportsStreamingImpl,
    };

    fn chatWithSystemImpl(
        _: *anyopaque,
        allocator: std.mem.Allocator,
        _: ?[]const u8,
        _: []const u8,
        _: []const u8,
        _: f64,
    ) anyerror![]const u8 {
        return try allocator.dupe(u8, "");
    }

    fn chatImpl(
        ptr: *anyopaque,
        allocator: std.mem.Allocator,
        _: ChatRequest,
        _: []const u8,
        _: f64,
    ) anyerror!ChatResponse {
        const self: *StubProvider = @ptrCast(@alignCast(ptr));
        defer self.call_count += 1;
        if (self.call_count > 0) return emptyResponse(allocator);
        return cannedResponse(allocator);
    }

    fn supportsNativeToolsImpl(_: *anyopaque) bool {
        return true;
    }

    fn supportsStreamingImpl(_: *anyopaque) bool {
        return true;
    }

    fn getNameImpl(_: *anyopaque) []const u8 {
        return "stub";
    }

    fn deinitImpl(_: *anyopaque) void {}
};

fn emptyResponse(allocator: std.mem.Allocator) !ChatResponse {
    return ChatResponse{
        .content = try allocator.dupe(u8, ""),
        .tool_calls = &.{},
        .usage = TokenUsage{},
        .provider = try allocator.dupe(u8, "stub"),
        .model = "stub",
    };
}

fn cannedResponse(allocator: std.mem.Allocator) !ChatResponse {
    const content = try allocator.dupe(u8, CONTENT_TEMPLATE);
    errdefer allocator.free(content);

    var calls = try allocator.alloc(ToolCall, 1);
    errdefer allocator.free(calls);

    const id = try allocator.dupe(u8, CANNED_TOOL_CALL_ID);
    errdefer allocator.free(id);
    const name = try allocator.dupe(u8, CANNED_TOOL_NAME);
    errdefer allocator.free(name);
    const args = try allocator.dupe(u8, ARGS_TEMPLATE);
    errdefer allocator.free(args);

    calls[0] = .{ .id = id, .name = name, .arguments = args };
    return ChatResponse{
        .content = content,
        .tool_calls = calls,
        .usage = TokenUsage{ .prompt_tokens = 8, .completion_tokens = 16, .total_tokens = 24 },
        .provider = try allocator.dupe(u8, "stub"),
        .model = "stub",
    };
}
