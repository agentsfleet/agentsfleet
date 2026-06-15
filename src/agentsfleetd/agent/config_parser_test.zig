const std = @import("std");
const config_parser = @import("config_parser.zig");
const config_types = @import("config_types.zig");

const parseAgentConfig = config_parser.parseAgentConfig;
const AgentConfigError = config_types.AgentConfigError;

test "parseAgentConfig: valid config parses all fields" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"platform-ops",
        \\ "x-agentsfleet":{
        \\   "triggers":[{"type":"webhook","source":"agentmail","events":["message.received"]}],
        \\   "tools":["agentmail"],"credentials":["agentmail_api_key"],
        \\   "network":{"allow":["api.agentmail.to"]},"budget":{"daily_dollars":5.0}
        \\ }}
    ;
    var cfg = try parseAgentConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("platform-ops", cfg.name);
    try std.testing.expectEqual(@as(usize, 1), cfg.triggers.len);
    try std.testing.expectEqualStrings("agentmail", cfg.triggers[0].webhook.source);
    try std.testing.expectEqualStrings("message.received", cfg.triggers[0].webhook.events.?[0]);
    try std.testing.expectApproxEqAbs(@as(f64, 5.0), cfg.budget.daily_dollars, 0.001);
    try std.testing.expect(cfg.skill == null);
}

test "parseAgentConfig: missing name returns MissingRequiredField" {
    const alloc = std.testing.allocator;
    const json =
        \\{"x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],
        \\ "tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(AgentConfigError.MissingRequiredField, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: invalid trigger type returns InvalidTriggerType" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{"triggers":[{"type":"invalid"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(AgentConfigError.InvalidTriggerType, parseAgentConfig(alloc, json));
}

// Regression: `chain` was a parser-accepted trigger type whose runtime had no
// matching EventType variant — meaning a config could declare `type: chain`
// and the runtime would silently never deliver an event. The chain branch was
// removed; this test pins the rejection so anyone re-adding the branch (e.g.
// to wire chained execution) gets a failing test forcing them to confirm the
// EventType + writepath consumers exist before re-introducing the config.
test "parseAgentConfig: chain trigger type rejected as InvalidTriggerType" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{"triggers":[{"type":"chain","source":"upstream-agent"}],
        \\ "tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(AgentConfigError.InvalidTriggerType, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: skill field parsed from runtime block" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"enricher",
        \\ "x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],
        \\   "tools":["agentmail"],"skill":"clawhub://queen/lead-hunter@1.0.1",
        \\   "budget":{"daily_dollars":2.0}}}
    ;
    var cfg = try parseAgentConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("clawhub://queen/lead-hunter@1.0.1", cfg.skill.?);
}

test "parseAgentConfig: cron trigger defaults" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"nightly",
        \\ "x-agentsfleet":{"triggers":[{"type":"cron","schedule":"0 3 * * *"}],
        \\   "tools":["agentmail"],"budget":{"daily_dollars":0.5}}}
    ;
    var cfg = try parseAgentConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqual(@as(usize, 1), cfg.triggers.len);
    try std.testing.expectEqualStrings("0 3 * * *", cfg.triggers[0].cron.schedule);
    try std.testing.expectEqual(@as(usize, 0), cfg.credentials.len);
    try std.testing.expect(cfg.network == null);
    try std.testing.expect(cfg.gates == null);
}

test "parseAgentConfig: api trigger is rejected with InvalidTriggerType" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"api-agent",
        \\ "x-agentsfleet":{"triggers":[{"type":"api"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(AgentConfigError.InvalidTriggerType, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: singular trigger key inside x-agentsfleet returns UnknownRuntimeKey" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{
        \\  "trigger":{"type":"webhook","source":"github"},
        \\  "tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(AgentConfigError.UnknownRuntimeKey, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: triggers array parsed under x-agentsfleet" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{
        \\  "triggers":[
        \\    {"type":"webhook","source":"github","events":["workflow_run"]},
        \\    {"type":"cron","schedule":"0 3 * * *"}
        \\  ],
        \\  "tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    var cfg = try parseAgentConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqual(@as(usize, 2), cfg.triggers.len);
    try std.testing.expectEqualStrings("github", cfg.triggers[0].webhook.source);
    try std.testing.expectEqualStrings("workflow_run", cfg.triggers[0].webhook.events.?[0]);
    try std.testing.expectEqualStrings("0 3 * * *", cfg.triggers[1].cron.schedule);
}

test "parseAgentConfig: malformed JSON returns MissingRequiredField" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(AgentConfigError.MissingRequiredField, parseAgentConfig(alloc, "not json"));
}

test "parseAgentConfig: root is array not object returns MissingRequiredField" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(AgentConfigError.MissingRequiredField, parseAgentConfig(alloc, "[]"));
}

test "parseAgentConfig: empty tools array rejected" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{"triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":[],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(AgentConfigError.MissingRequiredField, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: partial-build leak check (invalid budget after valid tools)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x",
        \\ "x-agentsfleet":{"triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],
        \\   "credentials":["ok_cred"],"budget":{"daily_dollars":-1.0}}}
    ;
    try std.testing.expectError(AgentConfigError.InvalidBudget, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: tools at top level returns RuntimeKeysOutsideBlock" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","tools":["agentmail"],
        \\ "x-agentsfleet":{"triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(AgentConfigError.RuntimeKeysOutsideBlock, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: gates at top level returns RuntimeKeysOutsideBlock" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","gates":{"daily":{"max":1}},"x-agentsfleet":{"triggers":[{"type":"cron","schedule":"0 0 * * *"}],
        \\ "tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(AgentConfigError.RuntimeKeysOutsideBlock, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: skill at top level returns RuntimeKeysOutsideBlock" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","skill":"clawhub://q/s@1","x-agentsfleet":{"triggers":[{"type":"cron","schedule":"0 0 * * *"}],
        \\ "tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(AgentConfigError.RuntimeKeysOutsideBlock, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: budget at top level returns RuntimeKeysOutsideBlock" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","budget":{"daily_dollars":1.0},
        \\ "x-agentsfleet":{"triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(AgentConfigError.RuntimeKeysOutsideBlock, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: plural triggers at top level returns RuntimeKeysOutsideBlock" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","triggers":[{"type":"webhook","source":"github"}],
        \\ "x-agentsfleet":{"triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(AgentConfigError.RuntimeKeysOutsideBlock, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: missing x-agentsfleet block returns UseagentBlockRequired" {
    const alloc = std.testing.allocator;
    const json = "{\"name\":\"x\"}";
    try std.testing.expectError(AgentConfigError.UseagentBlockRequired, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: x-agentsfleet present but not an object returns UseagentBlockRequired" {
    const alloc = std.testing.allocator;
    const json = "{\"name\":\"x\",\"x-agentsfleet\":\"oops-string-not-object\"}";
    try std.testing.expectError(AgentConfigError.UseagentBlockRequired, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: typo under x-agentsfleet returns UnknownRuntimeKey" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{"triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],
        \\ "budget":{"daily_dollars":1.0},"contxt":{"foo":"bar"}}}
    ;
    try std.testing.expectError(AgentConfigError.UnknownRuntimeKey, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: unknown top-level key passes (permissive top level)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","tags":["foo"],"x-amp":{"v":1},
        \\ "x-agentsfleet":{"triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    var cfg = try parseAgentConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("x", cfg.name);
}

test "parseAgentConfig: x-agentsfleet.model populates AgentConfig.model verbatim" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{
        \\  "triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "model":"accounts/fireworks/models/kimi-k2.6"}}
    ;
    var cfg = try parseAgentConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("accounts/fireworks/models/kimi-k2.6", cfg.model.?);
}

test "parseAgentConfig: empty x-agentsfleet.model becomes null (self-managed sentinel)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{
        \\  "triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "model":""}}
    ;
    var cfg = try parseAgentConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expect(cfg.model == null);
}

test "parseAgentConfig: x-agentsfleet.context populates every knob" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{
        \\  "triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "context":{"context_cap_tokens":256000,"tool_window":30,"memory_checkpoint_every":7,"stage_chunk_threshold":0.8}}}
    ;
    var cfg = try parseAgentConfig(alloc, json);
    defer cfg.deinit(alloc);
    const ctx = cfg.context.?;
    try std.testing.expectEqual(@as(u32, 256000), ctx.context_cap_tokens);
    try std.testing.expectEqual(@as(u32, 30), ctx.tool_window);
    try std.testing.expectEqual(@as(u32, 7), ctx.memory_checkpoint_every);
    try std.testing.expectEqual(@as(f32, 0.8), ctx.stage_chunk_threshold);
}

test "parseAgentConfig: tool_window auto-string maps to 0 (auto-sentinel)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{
        \\  "triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "context":{"tool_window":"auto"}}}
    ;
    var cfg = try parseAgentConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqual(@as(u32, 0), cfg.context.?.tool_window);
}

test "parseAgentConfig: missing context block returns null (auto downstream)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{
        \\  "triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    var cfg = try parseAgentConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expect(cfg.context == null);
    try std.testing.expect(cfg.model == null);
}

test "parseAgentConfig: context with non-numeric tool_window returns InvalidFieldType (not MissingRequiredField)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{
        \\  "triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "context":{"tool_window":true}}}
    ;
    // Distinguishes "you forgot a key" (MissingRequiredField) from "you got
    // the shape wrong" (InvalidFieldType). A future author reading a CI log
    // shouldn't waste time hunting for a missing field that's actually
    // present-but-mistyped.
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: negative tool_window returns InvalidFieldType" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{
        \\  "triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "context":{"tool_window":-1}}}
    ;
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: context block as string (not object) returns InvalidFieldType" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{
        \\  "triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "context":"oops-not-an-object"}}
    ;
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: model field as integer (not string) returns InvalidFieldType" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{
        \\  "triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "model":42}}
    ;
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: tool_window string other than 'auto' returns InvalidFieldType (not silently coerced)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{
        \\  "triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "context":{"tool_window":"AUTO"}}}
    ;
    // Tight contract: the auto-sentinel is exactly "auto" — case-sensitive,
    // no trimming, no synonyms. Anything else fails loud rather than
    // silently coercing to 0.
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: model at top level returns RuntimeKeysOutsideBlock" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","model":"oops",
        \\ "x-agentsfleet":{"triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(AgentConfigError.RuntimeKeysOutsideBlock, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: typo inside x-agentsfleet.context returns UnknownRuntimeKey (not silent default)" {
    const alloc = std.testing.allocator;
    // `tool_windw` (typo, missing 'o') — without the guard, this silently
    // falls through to the zero auto-sentinel and the operator's intended
    // override of 30 is dropped at runtime. Catching it at install time
    // surfaces the typo where the operator can fix it.
    const json =
        \\{"name":"x","x-agentsfleet":{
        \\  "triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "context":{"tool_windw":30}}}
    ;
    try std.testing.expectError(AgentConfigError.UnknownRuntimeKey, parseAgentConfig(alloc, json));
}

test "parseAgentConfig: every documented context key accepted (no false positives from the typo guard)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-agentsfleet":{
        \\  "triggers":[{"type":"cron","schedule":"0 0 * * *"}],"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "context":{
        \\    "context_cap_tokens":256000,
        \\    "tool_window":30,
        \\    "memory_checkpoint_every":7,
        \\    "stage_chunk_threshold":0.8
        \\  }}}
    ;
    var cfg = try parseAgentConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqual(@as(u32, 30), cfg.context.?.tool_window);
}
