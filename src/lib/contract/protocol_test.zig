//! Round-trip serialization proof for the frozen /v1/runners protocol: every
//! request/response type and enum serializes to JSON and parses back to a value
//! that re-serializes identically. Stability (stringify → parse → stringify
//! equality) is the equality check — it covers the std.json.Value secrets_map
//! without a hand-rolled deep compare.

const std = @import("std");
const protocol = @import("protocol.zig");
const FailureClass = @import("execution_result.zig").FailureClass;
const MS_PER_SECOND = 1000;

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
    const assigned = protocol.AssignedPolicy{
        .sandbox_tier = .landlock_full,
        .network_policy = .allow_all,
        .registry_allowlist = &.{"registry.npmjs.org"},
        .worker_count = 2,
    };
    try expectStable(protocol.RegisterRequest, .{
        .host_id = "host-01",
        .assigned_policy = assigned,
        .labels = &.{ "linux", "gpu" },
    });
    try expectStable(protocol.RegisterResponse, .{
        .runner_id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee",
        .runner_token = "rt_secret",
        .assigned_policy = assigned,
    });
}

test "runner admin patch bodies round-trip in both one-of shapes" {
    try expectStable(protocol.RunnerAdminPatchRequest, .{ .action = .cordon });
    try expectStable(protocol.RunnerAdminPatchRequest, .{ .assigned_policy = .{
        .sandbox_tier = .container_nested,
        .network_policy = .deny_all_egress,
        .registry_allowlist = &.{},
        .worker_count = 1,
    } });
    try expectStable(protocol.RunnerAdminPatchResponse, .{
        .id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee",
        .admin_state = .active,
        .assigned_policy = .{
            .sandbox_tier = .landlock_full,
            .network_policy = .allow_all,
            .registry_allowlist = &.{"pypi.org"},
            .worker_count = 4,
        },
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
        // The cumulative split the report-settle meters the final slice off.
        .input_tokens = 900,
        .cached_input_tokens = 200,
        .output_tokens = 134,
        .telemetry = .{ .time_to_first_token_ms = 42, .wall_ms = 1500 },
        .checkpoint = .{ .last_event_id = "1700000000000-0", .last_response = "ok" },
    });
    try expectStable(protocol.ReportResponse, .{ .ok = true });
}

test "report request carries the granular failure_reason across the round-trip" {
    inline for (.{ FailureClass.oom_kill, FailureClass.renewal_terminate, FailureClass.timeout_kill }) |fc| {
        try expectStable(protocol.ReportRequest, .{
            .lease_id = "lease_0190aaaa",
            .event_id = "1700000000000-0",
            .fencing_token = 184,
            .outcome = .fleet_error,
            .failure_reason = fc,
            .response_text = "killed",
            .tokens = 0,
            .telemetry = .{ .time_to_first_token_ms = 0, .wall_ms = 1500 },
            .checkpoint = .{ .last_event_id = "1700000000000-0", .last_response = "" },
        });
    }
}

test "report request without failure_reason parses to null (old runner, backward-additive)" {
    const a = std.testing.allocator;
    // A report emitted by an OLD runner — no failure_reason key. The new control
    // plane must still parse it, defaulting the field to null (rendered downstream
    // as the "unknown" failure bucket, never a parse failure).
    const json_old =
        \\{"lease_id":"l1","event_id":"1700000000000-0","fencing_token":1,"outcome":"fleet_error","response_text":"x","tokens":0,"telemetry":{"time_to_first_token_ms":0,"wall_ms":10},"checkpoint":{"last_event_id":"1700000000000-0","last_response":""}}
    ;
    const p = try std.json.parseFromSlice(protocol.ReportRequest, a, json_old, .{});
    defer p.deinit();
    try std.testing.expect(p.value.failure_reason == null);
    // The cumulative token split is also additive — an old report omits it and
    // settles run-fee-only off all-zero cumulatives (never a parse failure).
    try std.testing.expectEqual(@as(u32, 0), p.value.input_tokens);
    try std.testing.expectEqual(@as(u32, 0), p.value.cached_input_tokens);
    try std.testing.expectEqual(@as(u32, 0), p.value.output_tokens);
}

test "renew request round-trips its cumulative token counts" {
    try expectStable(protocol.RenewRequest, .{
        .input_tokens = 12_000,
        .cached_input_tokens = 3_400,
        .output_tokens = 5_600,
    });
}

test "renew request from an empty body parses to all-zero cumulatives (old runner / pre-accounting /renew)" {
    const a = std.testing.allocator;
    // A /renew before token accounting wires into the runner — or an older
    // runner — sends an empty body. The control plane must parse it to all-zero
    // cumulatives → run-fee-only metering, never a parse failure (and so never a
    // negative Δ once the cursor subtraction runs in the renewal CTE).
    const json_empty = "{}";
    const p = try std.json.parseFromSlice(protocol.RenewRequest, a, json_empty, .{});
    defer p.deinit();
    try std.testing.expectEqual(@as(u32, 0), p.value.input_tokens);
    try std.testing.expectEqual(@as(u32, 0), p.value.cached_input_tokens);
    try std.testing.expectEqual(@as(u32, 0), p.value.output_tokens);
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
                .fleet_id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee",
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
                .repository_binding = .{
                    .repositories = &.{"acme/payments"},
                    .access = .write,
                    .base_branch = "main",
                },
                .http_origin_policies = &.{.{
                    .host = "api.code.example",
                    .credential_names = &.{"source_control"},
                    .requests = &.{.{
                        .method = .post,
                        .path = "/projects/acme/payments/reviews",
                        .json_fields = &.{.{ .name = "draft", .boolean_value = true }},
                    }},
                }},
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

test "lease read-only HTTP restrictions survive the runner wire" {
    const a = std.testing.allocator;
    const response = protocol.LeaseResponse{
        .lease = .{
            .lease_id = "lease_0190aaaa",
            .fencing_token = 184,
            .lease_expires_at = 1700000030000,
            .secret_delivery = .@"inline",
            .event = .{
                .event_id = "1700000000000-0",
                .fleet_id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee",
                .workspace_id = "0190cccc-dddd-7eee-8fff-aaaaaaaaaaaa",
                .actor = "system:repair-verifier",
                .event_type = .webhook,
                .request_json = "{}",
                .created_at = 1700000000000,
            },
            .policy = .{
                .network_policy = .{
                    .allow = &.{ "api.github.com", "elastic.example.com" },
                    .read_only = true,
                    .read_post_paths = &.{"https://elastic.example.com/_query"},
                },
            },
        },
    };
    const json = try std.json.Stringify.valueAlloc(a, response, .{});
    defer a.free(json);
    const parsed = try std.json.parseFromSlice(protocol.LeaseResponse, a, json, .{});
    defer parsed.deinit();
    const policy = parsed.value.lease.?.policy.network_policy;
    try std.testing.expect(policy.read_only);
    try std.testing.expectEqual(@as(usize, 1), policy.read_post_paths.len);
    try std.testing.expectEqualStrings("https://elastic.example.com/_query", policy.read_post_paths[0]);
}

test "lease response — no-work carries a backoff hint" {
    try expectStable(protocol.LeaseResponse, .{ .lease = null, .retry_after_ms = MS_PER_SECOND });
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
                .fleet_id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee",
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
    // A lease emitted by an OLD agentsfleetd — no provider/api_key keys on the policy.
    // The new runner must still parse it, defaulting both fields to "" (no key,
    // surfaces downstream as a clean engine config error, never a parse failure).
    const json_old =
        \\{"lease":{"lease_id":"l1","fencing_token":1,"lease_expires_at":1700000030000,"secret_delivery":"inline","event":{"event_id":"1700000000000-0","fleet_id":"0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee","workspace_id":"0190cccc-dddd-7eee-8fff-aaaaaaaaaaaa","actor":"steer:kishore","event_type":"webhook","request_json":"{}","created_at":1700000000000},"policy":{"network_policy":{"allow":[]},"tools":[],"secrets_map":null,"context":{"tool_window":20,"memory_checkpoint_every":5,"stage_chunk_threshold":0.75,"model":"m","context_cap_tokens":200000}}},"retry_after_ms":null}
    ;
    const p = try std.json.parseFromSlice(protocol.LeaseResponse, a, json_old, .{ .ignore_unknown_fields = true });
    defer p.deinit();
    try std.testing.expectEqualStrings("", p.value.lease.?.policy.provider);
    try std.testing.expectEqualStrings("", p.value.lease.?.policy.api_key);
}

test "lease payload carries the installed instructions across the round-trip" {
    try expectStable(protocol.LeaseResponse, .{
        .lease = .{
            .lease_id = "l1",
            .fencing_token = 1,
            .lease_expires_at = 1700000030000,
            .secret_delivery = .@"inline",
            .event = .{
                .event_id = "1700000000000-0",
                .fleet_id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee",
                .workspace_id = "0190cccc-dddd-7eee-8fff-aaaaaaaaaaaa",
                .actor = "steer:kishore",
                .event_type = .chat,
                .request_json = "{}",
                .created_at = 1700000000000,
            },
            .policy = .{ .context = .{ .model = "m" } },
            .instructions = "Do platform ops: fetch logs, correlate, post diagnosis.",
        },
        .retry_after_ms = null,
    });
}

test "lease payload without instructions parses to empty default (backward-additive)" {
    const a = std.testing.allocator;
    // A lease emitted by an OLD agentsfleetd — no `instructions` key. The new runner
    // must parse it, defaulting to "" (the runner then renders an explicit
    // no-instructions sentinel). Rollout is runners-first, so an OLD runner never
    // receives a NEW lease carrying the field.
    const json_old =
        \\{"lease":{"lease_id":"l1","fencing_token":1,"lease_expires_at":1700000030000,"secret_delivery":"inline","event":{"event_id":"1700000000000-0","fleet_id":"0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee","workspace_id":"0190cccc-dddd-7eee-8fff-aaaaaaaaaaaa","actor":"steer:kishore","event_type":"webhook","request_json":"{}","created_at":1700000000000},"policy":{"network_policy":{"allow":[]},"tools":[],"secrets_map":null,"context":{"tool_window":20,"memory_checkpoint_every":5,"stage_chunk_threshold":0.75,"model":"m","context_cap_tokens":200000}}},"retry_after_ms":null}
    ;
    const p = try std.json.parseFromSlice(protocol.LeaseResponse, a, json_old, .{ .ignore_unknown_fields = true });
    defer p.deinit();
    try std.testing.expectEqualStrings("", p.value.lease.?.instructions);
}

test "lease response carries an inline secrets_map across the round-trip" {
    const a = std.testing.allocator;
    const json_in =
        \\{"lease":{"lease_id":"lease_0190aaaa","fencing_token":184,"lease_expires_at":1700000030000,"secret_delivery":"inline","event":{"event_id":"1700000000000-0","fleet_id":"0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee","workspace_id":"0190cccc-dddd-7eee-8fff-aaaaaaaaaaaa","actor":"steer:kishore","event_type":"webhook","request_json":"{}","created_at":1700000000000},"policy":{"network_policy":{"allow":["api.github.com"]},"tools":["bash"],"secrets_map":{"github":{"token":"ghp_x"}},"context":{"tool_window":20,"memory_checkpoint_every":5,"stage_chunk_threshold":0.75,"model":"claude-opus-4-7","context_cap_tokens":200000}}},"retry_after_ms":null}
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

test "assigned-policy vocabulary round-trips via tag names" {
    inline for (std.meta.fields(protocol.NetworkPolicy)) |f| {
        try expectStable(protocol.NetworkPolicy, @field(protocol.NetworkPolicy, f.name));
    }
    try expectStable(protocol.AssignedPolicy, .{
        .sandbox_tier = .landlock_full,
        .network_policy = .allow_all,
        .registry_allowlist = &.{ "registry.npmjs.org", "pypi.org" },
        .worker_count = 4,
    });
    try expectStable(protocol.CapabilityReport, .{
        .landlock = true,
        .seccomp = true,
        .cgroup_controllers = &.{ "cpu", "memory", "pids" },
        .bubblewrap = false,
        .egress_enforcement = false,
    });
}

test "heartbeat request: an empty body parses to no capability report (mixed-version safety)" {
    const a = std.testing.allocator;
    const p = try std.json.parseFromSlice(protocol.HeartbeatRequest, a, "{}", .{});
    defer p.deinit();
    try std.testing.expect(p.value.capability_report == null);
    try expectStable(protocol.HeartbeatRequest, .{ .capability_report = .{
        .landlock = true,
        .seccomp = true,
        .cgroup_controllers = &.{"cpu"},
        .bubblewrap = true,
        .egress_enforcement = false,
    } });
}

test "heartbeat and self replies carry the assignment and the degraded verdict" {
    const assigned = protocol.AssignedPolicy{
        .sandbox_tier = .landlock_full,
        .network_policy = .deny_all_egress,
        .registry_allowlist = &.{},
        .worker_count = 1,
    };
    try expectStable(protocol.HeartbeatResponse, .{
        .status = .ok,
        .assigned_policy = assigned,
        .degraded = true,
        .degraded_reason = "cgroup controllers not delegated",
    });
    // A bare-status reply (the pre-policy shape) still parses — fields default.
    const a = std.testing.allocator;
    const p = try std.json.parseFromSlice(protocol.HeartbeatResponse, a,
        \\{"status":"ok"}
    , .{});
    defer p.deinit();
    try std.testing.expect(p.value.assigned_policy == null);
    try std.testing.expect(!p.value.degraded);
    try expectStable(protocol.SelfResponse, .{
        .id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee",
        .status = "active",
        .host_id = "host-01",
        .sandbox_tier = "landlock_full",
        .last_seen_at = 0,
        .assigned_policy = assigned,
        .achievable = null,
        .degraded = false,
        .degraded_reason = null,
    });
}

test "worker-pool bounds are sane; the fail-closed egress default is never open" {
    try std.testing.expect(protocol.MIN_WORKER_COUNT <= protocol.DEFAULT_WORKER_COUNT);
    try std.testing.expect(protocol.DEFAULT_WORKER_COUNT <= protocol.MAX_WORKER_COUNT);
    try std.testing.expect(protocol.FAIL_CLOSED_DEFAULT != .allow_all);
    try std.testing.expect(!protocol.FAIL_CLOSED_DEFAULT.sharesHostNet());
}
