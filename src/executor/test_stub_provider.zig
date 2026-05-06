//! Test-only NullClaw provider that emits canned responses.
//!
//! Compiled into `zombied-executor-stub` only — production and harness
//! binaries strip this module via the `executor_provider_stub` build option
//! comptime branch in runner.zig.
//!
//! Slice 1 ships a minimal "no work to do" response so the executor pipeline
//! (NullClaw + observer adapter + redactor) runs end-to-end without spending
//! tokens. Slice 2 swaps the body for a canned tool_use referencing
//! `${secrets.fly.api_token}` — that is what the redaction harness asserts.

const std = @import("std");
const nullclaw = @import("nullclaw");

const providers = nullclaw.providers;
const Provider = providers.Provider;
const ChatRequest = providers.ChatRequest;
const ChatResponse = providers.ChatResponse;
const TokenUsage = providers.TokenUsage;

pub const StubProvider = struct {
    allocator: std.mem.Allocator,

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
    };

    fn chatWithSystemImpl(
        ptr: *anyopaque,
        allocator: std.mem.Allocator,
        _: ?[]const u8,
        _: []const u8,
        _: []const u8,
        _: f64,
    ) anyerror![]const u8 {
        _ = ptr;
        return try allocator.dupe(u8, "");
    }

    fn chatImpl(
        ptr: *anyopaque,
        _: std.mem.Allocator,
        _: ChatRequest,
        _: []const u8,
        _: f64,
    ) anyerror!ChatResponse {
        _ = ptr;
        return ChatResponse{
            .content = "",
            .tool_calls = &.{},
            .usage = TokenUsage{},
            .provider = "stub",
            .model = "stub",
        };
    }

    fn supportsNativeToolsImpl(_: *anyopaque) bool {
        return false;
    }

    fn getNameImpl(_: *anyopaque) []const u8 {
        return "stub";
    }

    fn deinitImpl(_: *anyopaque) void {}
};
