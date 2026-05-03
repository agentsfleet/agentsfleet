const std = @import("std");
const config_parser = @import("config_parser.zig");
const config_types = @import("config_types.zig");

const parseZombieConfig = config_parser.parseZombieConfig;
const ZombieConfigError = config_types.ZombieConfigError;

test "parseZombieConfig: valid config parses all fields" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"lead-collector",
        \\ "x-usezombie":{
        \\   "trigger":{"type":"webhook","source":"agentmail","event":"message.received"},
        \\   "tools":["agentmail"],"credentials":["agentmail_api_key"],
        \\   "network":{"allow":["api.agentmail.to"]},"budget":{"daily_dollars":5.0},
        \\   "chain":["lead-enricher"]
        \\ }}
    ;
    var cfg = try parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("lead-collector", cfg.name);
    try std.testing.expectEqualStrings("agentmail", cfg.trigger.webhook.source);
    try std.testing.expectApproxEqAbs(@as(f64, 5.0), cfg.budget.daily_dollars, 0.001);
    try std.testing.expectEqual(@as(usize, 1), cfg.chain.len);
    try std.testing.expectEqualStrings("lead-enricher", cfg.chain[0]);
    try std.testing.expect(cfg.skill == null);
}

test "parseZombieConfig: missing name returns MissingRequiredField" {
    const alloc = std.testing.allocator;
    const json =
        \\{"x-usezombie":{"trigger":{"type":"webhook","source":"agentmail"},
        \\ "tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(ZombieConfigError.MissingRequiredField, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: invalid trigger type returns InvalidTriggerType" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{"trigger":{"type":"invalid"},"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(ZombieConfigError.InvalidTriggerType, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: skill field parsed from runtime block" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"enricher",
        \\ "x-usezombie":{"trigger":{"type":"chain","source":"lead-collector"},
        \\   "tools":["agentmail"],"skill":"clawhub://queen/lead-hunter@1.0.1",
        \\   "budget":{"daily_dollars":2.0}}}
    ;
    var cfg = try parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("clawhub://queen/lead-hunter@1.0.1", cfg.skill.?);
    try std.testing.expectEqualStrings("lead-collector", cfg.trigger.chain.source);
}

test "parseZombieConfig: cron trigger + empty chain defaults" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"nightly",
        \\ "x-usezombie":{"trigger":{"type":"cron","schedule":"0 3 * * *"},
        \\   "tools":["agentmail"],"budget":{"daily_dollars":0.5}}}
    ;
    var cfg = try parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("0 3 * * *", cfg.trigger.cron.schedule);
    try std.testing.expectEqual(@as(usize, 0), cfg.chain.len);
    try std.testing.expectEqual(@as(usize, 0), cfg.credentials.len);
    try std.testing.expect(cfg.network == null);
    try std.testing.expect(cfg.gates == null);
}

test "parseZombieConfig: api trigger has no payload" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"api-agent",
        \\ "x-usezombie":{"trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    var cfg = try parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqual(config_types.ZombieTriggerType.api, @as(config_types.ZombieTriggerType, cfg.trigger));
}

test "parseZombieConfig: malformed JSON returns MissingRequiredField" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(ZombieConfigError.MissingRequiredField, parseZombieConfig(alloc, "not json"));
}

test "parseZombieConfig: root is array not object → MissingRequiredField" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(ZombieConfigError.MissingRequiredField, parseZombieConfig(alloc, "[]"));
}

test "parseZombieConfig: empty tools array rejected" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{"trigger":{"type":"api"},"tools":[],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(ZombieConfigError.MissingRequiredField, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: partial-build leak check (invalid budget after valid tools)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x",
        \\ "x-usezombie":{"trigger":{"type":"api"},"tools":["agentmail"],
        \\   "credentials":["ok_cred"],"budget":{"daily_dollars":-1.0}}}
    ;
    try std.testing.expectError(ZombieConfigError.InvalidBudget, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: tools at top level → RuntimeKeysOutsideBlock" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","tools":["agentmail"],
        \\ "x-usezombie":{"trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(ZombieConfigError.RuntimeKeysOutsideBlock, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: gates at top level → RuntimeKeysOutsideBlock" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","gates":{"daily":{"max":1}},"x-usezombie":{"trigger":{"type":"api"},
        \\ "tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(ZombieConfigError.RuntimeKeysOutsideBlock, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: skill at top level → RuntimeKeysOutsideBlock" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","skill":"clawhub://q/s@1","x-usezombie":{"trigger":{"type":"api"},
        \\ "tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(ZombieConfigError.RuntimeKeysOutsideBlock, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: chain at top level → RuntimeKeysOutsideBlock" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","chain":["downstream"],"x-usezombie":{"trigger":{"type":"api"},
        \\ "tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(ZombieConfigError.RuntimeKeysOutsideBlock, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: budget at top level → RuntimeKeysOutsideBlock" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","budget":{"daily_dollars":1.0},
        \\ "x-usezombie":{"trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(ZombieConfigError.RuntimeKeysOutsideBlock, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: missing x-usezombie block → UsezombieBlockRequired" {
    const alloc = std.testing.allocator;
    const json = "{\"name\":\"x\"}";
    try std.testing.expectError(ZombieConfigError.UsezombieBlockRequired, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: x-usezombie present but not an object → UsezombieBlockRequired" {
    const alloc = std.testing.allocator;
    const json = "{\"name\":\"x\",\"x-usezombie\":\"oops-string-not-object\"}";
    try std.testing.expectError(ZombieConfigError.UsezombieBlockRequired, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: typo under x-usezombie → UnknownRuntimeKey" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{"trigger":{"type":"api"},"tools":["agentmail"],
        \\ "budget":{"daily_dollars":1.0},"contxt":{"foo":"bar"}}}
    ;
    try std.testing.expectError(ZombieConfigError.UnknownRuntimeKey, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: unknown top-level key passes (permissive top level)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","tags":["foo"],"x-amp":{"v":1},
        \\ "x-usezombie":{"trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    var cfg = try parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("x", cfg.name);
}

test "parseZombieConfig: x-usezombie.model populates ZombieConfig.model verbatim" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{
        \\  "trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "model":"accounts/fireworks/models/kimi-k2.6"}}
    ;
    var cfg = try parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("accounts/fireworks/models/kimi-k2.6", cfg.model.?);
}

test "parseZombieConfig: empty x-usezombie.model becomes null (BYOK sentinel)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{
        \\  "trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "model":""}}
    ;
    var cfg = try parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expect(cfg.model == null);
}

test "parseZombieConfig: x-usezombie.context populates every knob" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{
        \\  "trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "context":{"context_cap_tokens":256000,"tool_window":30,"memory_checkpoint_every":7,"stage_chunk_threshold":0.8}}}
    ;
    var cfg = try parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    const ctx = cfg.context.?;
    try std.testing.expectEqual(@as(u32, 256000), ctx.context_cap_tokens);
    try std.testing.expectEqual(@as(u32, 30), ctx.tool_window);
    try std.testing.expectEqual(@as(u32, 7), ctx.memory_checkpoint_every);
    try std.testing.expectEqual(@as(f32, 0.8), ctx.stage_chunk_threshold);
}

test "parseZombieConfig: tool_window auto-string maps to 0 (auto-sentinel)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{
        \\  "trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "context":{"tool_window":"auto"}}}
    ;
    var cfg = try parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqual(@as(u32, 0), cfg.context.?.tool_window);
}

test "parseZombieConfig: missing context block → null (auto downstream)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{
        \\  "trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    var cfg = try parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expect(cfg.context == null);
    try std.testing.expect(cfg.model == null);
}

test "parseZombieConfig: context with non-numeric tool_window → InvalidFieldType (not MissingRequiredField)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{
        \\  "trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "context":{"tool_window":true}}}
    ;
    // Distinguishes "you forgot a key" (MissingRequiredField) from "you got
    // the shape wrong" (InvalidFieldType). A future author reading a CI log
    // shouldn't waste time hunting for a missing field that's actually
    // present-but-mistyped.
    try std.testing.expectError(ZombieConfigError.InvalidFieldType, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: negative tool_window → InvalidFieldType" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{
        \\  "trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "context":{"tool_window":-1}}}
    ;
    try std.testing.expectError(ZombieConfigError.InvalidFieldType, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: context block as string (not object) → InvalidFieldType" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{
        \\  "trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "context":"oops-not-an-object"}}
    ;
    try std.testing.expectError(ZombieConfigError.InvalidFieldType, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: model field as integer (not string) → InvalidFieldType" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{
        \\  "trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "model":42}}
    ;
    try std.testing.expectError(ZombieConfigError.InvalidFieldType, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: tool_window string other than 'auto' → InvalidFieldType (not silently coerced)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","x-usezombie":{
        \\  "trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0},
        \\  "context":{"tool_window":"AUTO"}}}
    ;
    // Tight contract: the auto-sentinel is exactly "auto" — case-sensitive,
    // no trimming, no synonyms. Anything else fails loud rather than
    // silently coercing to 0.
    try std.testing.expectError(ZombieConfigError.InvalidFieldType, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: model at top level → RuntimeKeysOutsideBlock" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","model":"oops",
        \\ "x-usezombie":{"trigger":{"type":"api"},"tools":["agentmail"],"budget":{"daily_dollars":1.0}}}
    ;
    try std.testing.expectError(ZombieConfigError.RuntimeKeysOutsideBlock, parseZombieConfig(alloc, json));
}
