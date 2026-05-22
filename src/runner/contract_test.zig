//! Round-trip serialization proof for the frozen /v1/runner contract: every
//! request/response type and enum serializes to JSON and parses back to a
//! value that re-serializes identically. Stability (stringify → parse →
//! stringify equality) is the equality check — it covers the std.json.Value
//! secrets_map without a hand-rolled deep compare.

const std = @import("std");
const contract = @import("contract.zig");

/// Assert serialize → parse → serialize is stable for `value`.
fn expectStable(comptime T: type, value: T) !void {
    const a = std.testing.allocator;
    const j1 = try std.json.Stringify.valueAlloc(a, value, .{});
    defer a.free(j1);
    const parsed = try std.json.parseFromSlice(T, a, j1, .{});
    defer parsed.deinit();
    const j2 = try std.json.Stringify.valueAlloc(a, parsed.value, .{});
    defer a.free(j2);
    try std.testing.expectEqualStrings(j1, j2);
}

test "runner contract enums round-trip via their tag names" {
    inline for (.{ contract.SandboxTier, contract.SecretDelivery, contract.Outcome, contract.HeartbeatStatus }) |E| {
        inline for (std.meta.fields(E)) |f| {
            try expectStable(E, @field(E, f.name));
        }
    }
}

test "register request and response round-trip" {
    try expectStable(contract.RegisterRequest, .{
        .enrollment_token = "enr_abc",
        .host_id = "host-01",
        .sandbox_tier = .macos_seatbelt,
        .labels = &.{ "linux", "gpu" },
    });
    try expectStable(contract.RegisterResponse, .{
        .runner_id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee",
        .runner_token = "rt_secret",
    });
}

test "heartbeat request and response round-trip" {
    try expectStable(contract.HeartbeatRequest, .{ .runner_id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee" });
    try expectStable(contract.HeartbeatResponse, .{ .status = .ok });
}

test "report request and response round-trip" {
    try expectStable(contract.ReportRequest, .{
        .runner_id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee",
        .event_id = "1700000000000-0",
        .outcome = .processed,
        .response_text = "done",
        .tokens = 1234,
        .telemetry = .{ .time_to_first_token_ms = 42, .wall_ms = 1500 },
        .checkpoint = .{ .last_event_id = "1700000000000-0", .last_response = "ok" },
    });
    try expectStable(contract.ReportResponse, .{ .ok = true });
}

test "lease response round-trips event envelope and execution policy without secrets" {
    try expectStable(contract.LeaseResponse, .{
        .event = .{
            .event_id = "1700000000000-0",
            .zombie_id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee",
            .workspace_id = "0190cccc-dddd-7eee-8fff-aaaaaaaaaaaa",
            .actor = "steer:kishore",
            .event_type = .chat,
            .request_json = "{\"message\":\"hi\"}",
            .created_at = 1700000000000,
        },
        .policy = .{
            .network_policy = .{ .allow = &.{"api.example.com"} },
            .tools = &.{"bash"},
            .secrets_map = null,
            .context = .{
                .tool_window = 20,
                .memory_checkpoint_every = 5,
                .stage_chunk_threshold = 0.75,
                .model = "claude-opus-4-7",
                .context_cap_tokens = 200000,
            },
        },
    });
}

test "lease response carries an inline secrets_map across the round-trip" {
    const a = std.testing.allocator;
    const json_in =
        \\{"event":{"event_id":"1700000000000-0","zombie_id":"0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee","workspace_id":"0190cccc-dddd-7eee-8fff-aaaaaaaaaaaa","actor":"steer:kishore","event_type":"webhook","request_json":"{}","created_at":1700000000000},"policy":{"network_policy":{"allow":["api.github.com"]},"tools":["bash"],"secrets_map":{"github":{"token":"ghp_x"}},"context":{"tool_window":20,"memory_checkpoint_every":5,"stage_chunk_threshold":0.75,"model":"claude-opus-4-7","context_cap_tokens":200000}}}
    ;
    const p1 = try std.json.parseFromSlice(contract.LeaseResponse, a, json_in, .{});
    defer p1.deinit();
    const j2 = try std.json.Stringify.valueAlloc(a, p1.value, .{});
    defer a.free(j2);
    const p2 = try std.json.parseFromSlice(contract.LeaseResponse, a, j2, .{});
    defer p2.deinit();
    const j3 = try std.json.Stringify.valueAlloc(a, p2.value, .{});
    defer a.free(j3);
    try std.testing.expectEqualStrings(j2, j3);
    try std.testing.expect(p2.value.policy.secrets_map != null);
}
