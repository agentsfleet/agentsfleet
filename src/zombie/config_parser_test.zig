const std = @import("std");
const config_parser = @import("config_parser.zig");
const config_types = @import("config_types.zig");

const parseZombieConfig = config_parser.parseZombieConfig;
const ZombieConfigError = config_types.ZombieConfigError;

test "parseZombieConfig: valid config parses all fields" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"lead-collector","trigger":{"type":"webhook","source":"agentmail","event":"message.received"},
        \\ "tools":["agentmail"],"credentials":["agentmail_api_key"],
        \\ "network":{"allow":["api.agentmail.to"]},"budget":{"daily_dollars":5.0},
        \\ "chain":["lead-enricher"]}
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
        \\{"trigger": {"type": "webhook", "source": "agentmail"}, "tools": ["agentmail"], "budget": {"daily_dollars": 1.0}}
    ;
    try std.testing.expectError(ZombieConfigError.MissingRequiredField, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: invalid trigger type returns InvalidTriggerType" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name": "x", "trigger": {"type": "invalid"}, "tools": ["agentmail"], "budget": {"daily_dollars": 1.0}}
    ;
    try std.testing.expectError(ZombieConfigError.InvalidTriggerType, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: skill field parsed from JSON" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"enricher","trigger":{"type":"chain","source":"lead-collector"},
        \\ "tools":["agentmail"],"skill":"clawhub://queen/lead-hunter@1.0.1",
        \\ "budget":{"daily_dollars":2.0}}
    ;
    var cfg = try parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("clawhub://queen/lead-hunter@1.0.1", cfg.skill.?);
    try std.testing.expectEqualStrings("lead-collector", cfg.trigger.chain.source);
}

test "parseZombieConfig: cron trigger + empty chain defaults" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"nightly","trigger":{"type":"cron","schedule":"0 3 * * *"},
        \\ "tools":["agentmail"],"budget":{"daily_dollars":0.5}}
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
        \\{"name":"api-agent","trigger":{"type":"api"},"tools":["agentmail"],
        \\ "budget":{"daily_dollars":1.0}}
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
        \\{"name":"x","trigger":{"type":"api"},"tools":[],"budget":{"daily_dollars":1.0}}
    ;
    try std.testing.expectError(ZombieConfigError.MissingRequiredField, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: partial-build leak check (invalid budget after valid tools)" {
    // Proves the errdefer chain in parseZombieConfig frees the already-duped
    // name / trigger / tools / credentials when a later field fails.
    // std.testing.allocator panics on leak, so a clean exit means pass.
    const alloc = std.testing.allocator;
    const json =
        \\{"name":"x","trigger":{"type":"api"},"tools":["agentmail"],
        \\ "credentials":["ok_cred"],"budget":{"daily_dollars":-1.0}}
    ;
    try std.testing.expectError(ZombieConfigError.InvalidBudget, parseZombieConfig(alloc, json));
}
