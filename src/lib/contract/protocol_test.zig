//! Round-trip serialization proof for the frozen /v1/runners protocol: every
//! request/response type and enum serializes to JSON and parses back to a value
//! that re-serializes identically. Stability (stringify → parse → stringify
//! equality) is the equality check — it covers the std.json.Value secrets_map
//! without a hand-rolled deep compare.

const std = @import("std");
const protocol = @import("protocol.zig");

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

test "runner protocol enums round-trip via their tag names" {
    inline for (.{ protocol.SandboxTier, protocol.SecretDelivery, protocol.Outcome, protocol.HeartbeatStatus }) |E| {
        inline for (std.meta.fields(E)) |f| {
            try expectStable(E, @field(E, f.name));
        }
    }
}

test "register request and response round-trip (no runner_id; token is in the header)" {
    try expectStable(protocol.RegisterRequest, .{
        .host_id = "host-01",
        .sandbox_tier = .macos_seatbelt,
        .labels = &.{ "linux", "gpu" },
    });
    try expectStable(protocol.RegisterResponse, .{
        .runner_id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee",
        .runner_token = "rt_secret",
    });
}

test "heartbeat response round-trips" {
    try expectStable(protocol.HeartbeatResponse, .{ .status = .ok });
}

test "report request and response round-trip (fenced, no runner_id)" {
    try expectStable(protocol.ReportRequest, .{
        .lease_id = "lease_0190aaaa",
        .event_id = "1700000000000-0",
        .fencing_token = 184,
        .outcome = .processed,
        .response_text = "done",
        .tokens = 1234,
        .telemetry = .{ .time_to_first_token_ms = 42, .wall_ms = 1500 },
        .checkpoint = .{ .last_event_id = "1700000000000-0", .last_response = "ok" },
    });
    try expectStable(protocol.ReportResponse, .{ .ok = true });
}

test "lease response — work payload round-trips (fencing + event + policy)" {
    try expectStable(protocol.LeaseResponse, .{
        .lease = .{
            .lease_id = "lease_0190aaaa",
            .fencing_token = 184,
            .lease_expires_at = 1700000030000,
            .secret_delivery = .@"inline",
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
        },
        .retry_after_ms = null,
    });
}

test "lease response — no-work carries a backoff hint" {
    try expectStable(protocol.LeaseResponse, .{ .lease = null, .retry_after_ms = 1000 });
}

test "lease policy carries the resolved provider and api_key across the round-trip" {
    try expectStable(protocol.LeaseResponse, .{
        .lease = .{
            .lease_id = "lease_0190aaaa",
            .fencing_token = 184,
            .lease_expires_at = 1700000030000,
            .secret_delivery = .@"inline",
            .event = .{
                .event_id = "1700000000000-0",
                .zombie_id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee",
                .workspace_id = "0190cccc-dddd-7eee-8fff-aaaaaaaaaaaa",
                .actor = "steer:kishore",
                .event_type = .chat,
                .request_json = "{}",
                .created_at = 1700000000000,
            },
            .policy = .{
                .provider = "fireworks",
                .api_key = "fw_secret_key",
                .context = .{ .model = "accounts/fireworks/models/kimi-k2.6", .context_cap_tokens = 256000 },
            },
        },
        .retry_after_ms = null,
    });
}

test "lease policy without provider or api_key fields parses to empty defaults (backward-additive)" {
    const a = std.testing.allocator;
    // A lease emitted by an OLD zombied — no provider/api_key keys on the policy.
    // The new runner must still parse it, defaulting both fields to "" (no key,
    // surfaces downstream as a clean engine config error, never a parse failure).
    const json_old =
        \\{"lease":{"lease_id":"l1","fencing_token":1,"lease_expires_at":1700000030000,"secret_delivery":"inline","event":{"event_id":"1700000000000-0","zombie_id":"0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee","workspace_id":"0190cccc-dddd-7eee-8fff-aaaaaaaaaaaa","actor":"steer:kishore","event_type":"webhook","request_json":"{}","created_at":1700000000000},"policy":{"network_policy":{"allow":[]},"tools":[],"secrets_map":null,"context":{"tool_window":20,"memory_checkpoint_every":5,"stage_chunk_threshold":0.75,"model":"m","context_cap_tokens":200000}}},"retry_after_ms":null}
    ;
    const p = try std.json.parseFromSlice(protocol.LeaseResponse, a, json_old, .{ .ignore_unknown_fields = true });
    defer p.deinit();
    try std.testing.expectEqualStrings("", p.value.lease.?.policy.provider);
    try std.testing.expectEqualStrings("", p.value.lease.?.policy.api_key);
}

test "lease response carries an inline secrets_map across the round-trip" {
    const a = std.testing.allocator;
    const json_in =
        \\{"lease":{"lease_id":"lease_0190aaaa","fencing_token":184,"lease_expires_at":1700000030000,"secret_delivery":"inline","event":{"event_id":"1700000000000-0","zombie_id":"0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee","workspace_id":"0190cccc-dddd-7eee-8fff-aaaaaaaaaaaa","actor":"steer:kishore","event_type":"webhook","request_json":"{}","created_at":1700000000000},"policy":{"network_policy":{"allow":["api.github.com"]},"tools":["bash"],"secrets_map":{"github":{"token":"ghp_x"}},"context":{"tool_window":20,"memory_checkpoint_every":5,"stage_chunk_threshold":0.75,"model":"claude-opus-4-7","context_cap_tokens":200000}}},"retry_after_ms":null}
    ;
    const p1 = try std.json.parseFromSlice(protocol.LeaseResponse, a, json_in, .{});
    defer p1.deinit();
    const j2 = try std.json.Stringify.valueAlloc(a, p1.value, .{});
    defer a.free(j2);
    const p2 = try std.json.parseFromSlice(protocol.LeaseResponse, a, j2, .{});
    defer p2.deinit();
    const j3 = try std.json.Stringify.valueAlloc(a, p2.value, .{});
    defer a.free(j3);
    try std.testing.expectEqualStrings(j2, j3);
    try std.testing.expect(p1.value.lease.?.policy.secrets_map != null);
}
