// Tests for the `triggers[]` array validation. Drives the validation matrix
// (length, ≤1 cron, dedup, per-trigger shape) end-to-end through
// `config_parser.parseAgentConfig` — the array-iteration helper is not
// part of the parser's public surface, so this file wraps each test input
// in a minimal valid config envelope.

const std = @import("std");

const config_helpers = @import("config_helpers.zig");
const config_parser = @import("config_parser.zig");
const config_types = @import("config_types.zig");

const AgentTrigger = config_types.AgentTrigger;
const AgentConfigError = config_types.AgentConfigError;

const test_alloc = std.testing.allocator;

fn parseTriggerForTest(src: []const u8) !AgentTrigger {
    const parsed = try std.json.parseFromSlice(std.json.Value, test_alloc, src, .{});
    defer parsed.deinit();
    return config_helpers.parseAgentTrigger(test_alloc, parsed.value.object);
}

fn freeTrigger(t: AgentTrigger) void {
    config_types.freeAgentTrigger(test_alloc, t);
}

// Wrap `src` (a JSON array of trigger objects) in a minimal valid config
// envelope so we can reach the array-level validation through the public
// `parseAgentConfig` entry. On success the caller owns just the triggers
// slice — the rest of the parsed config is freed before return.
fn parseTriggersForTest(src: []const u8) ![]const AgentTrigger {
    const wrapped = try std.fmt.allocPrint(
        test_alloc,
        \\{{"name":"t","x-agentsfleet":{{"triggers":{s},"tools":["x"],"budget":{{"daily_dollars":1.0}}}}}}
    ,
        .{src},
    );
    defer test_alloc.free(wrapped);
    var cfg = try config_parser.parseAgentConfig(test_alloc, wrapped);
    // Detach triggers so deinit doesn't free them; caller owns the slice.
    const trs = cfg.triggers;
    cfg.triggers = &.{};
    cfg.deinit(test_alloc);
    return trs;
}

fn freeTriggers(trs: []const AgentTrigger) void {
    for (trs) |t| config_types.freeAgentTrigger(test_alloc, t);
    test_alloc.free(trs);
}

test "parseWebhookSignature: defaults from github registry" {
    const src =
        \\{"type":"webhook","source":"github","signature":{"secret_ref":"gh_secret"}}
    ;
    const trig = try parseTriggerForTest(src);
    defer freeTrigger(trig);
    const sig = trig.webhook.signature.?;
    try std.testing.expectEqualStrings("x-hub-signature-256", sig.header);
    try std.testing.expectEqualStrings("sha256=", sig.prefix);
    try std.testing.expect(sig.ts_header == null);
    try std.testing.expectEqualStrings("gh_secret", sig.secret_ref);
}

test "parseWebhookSignature: defaults from linear registry" {
    const src =
        \\{"type":"webhook","source":"linear","signature":{"secret_ref":"ln_secret"}}
    ;
    const trig = try parseTriggerForTest(src);
    defer freeTrigger(trig);
    const sig = trig.webhook.signature.?;
    try std.testing.expectEqualStrings("linear-signature", sig.header);
    try std.testing.expectEqualStrings("", sig.prefix);
    try std.testing.expectEqualStrings("ln_secret", sig.secret_ref);
}

test "parseWebhookSignature: no signature block is admitted as null" {
    const src =
        \\{"type":"webhook","source":"agentmail"}
    ;
    const trig = try parseTriggerForTest(src);
    defer freeTrigger(trig);
    try std.testing.expect(trig.webhook.signature == null);
}

test "parseWebhookSignature: Jira custom scheme outside the registry" {
    const src =
        \\{"type":"webhook","source":"jira","signature":{"secret_ref":"j","header":"x-jira-hook-signature","prefix":"sha256=","ts_header":"x-jira-timestamp"}}
    ;
    const trig = try parseTriggerForTest(src);
    defer freeTrigger(trig);
    const sig = trig.webhook.signature.?;
    try std.testing.expectEqualStrings("x-jira-hook-signature", sig.header);
    try std.testing.expectEqualStrings("sha256=", sig.prefix);
    try std.testing.expectEqualStrings("x-jira-timestamp", sig.ts_header.?);
}

test "parseWebhookSignature: explicit header overrides registry" {
    const src =
        \\{"type":"webhook","source":"github","signature":{"secret_ref":"s","header":"x-custom-gh"}}
    ;
    const trig = try parseTriggerForTest(src);
    defer freeTrigger(trig);
    const sig = trig.webhook.signature.?;
    try std.testing.expectEqualStrings("x-custom-gh", sig.header);
    try std.testing.expectEqualStrings("sha256=", sig.prefix);
}

test "parseWebhookSignature: missing secret_ref is rejected" {
    const src =
        \\{"type":"webhook","source":"github","signature":{}}
    ;
    try std.testing.expectError(AgentConfigError.InvalidSignatureConfig, parseTriggerForTest(src));
}

test "parseWebhookSignature: empty secret_ref is rejected" {
    const src =
        \\{"type":"webhook","source":"github","signature":{"secret_ref":""}}
    ;
    try std.testing.expectError(AgentConfigError.InvalidSignatureConfig, parseTriggerForTest(src));
}

test "parseWebhookSignature: unknown source without header is rejected" {
    const src =
        \\{"type":"webhook","source":"custom_provider","signature":{"secret_ref":"s"}}
    ;
    try std.testing.expectError(AgentConfigError.InvalidSignatureConfig, parseTriggerForTest(src));
}

test "parseWebhookSignature: header over MAX_SIGNATURE_HEADER_LEN is rejected" {
    // 65 'a's — one over MAX_SIGNATURE_HEADER_LEN (64).
    const src =
        \\{"type":"webhook","source":"jira","signature":{"secret_ref":"s","header":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}
    ;
    try std.testing.expectError(AgentConfigError.InvalidSignatureConfig, parseTriggerForTest(src));
}

// ── events allow-list (per-element + array shape) ───────────────────────

test "parseAgentTrigger.webhook: events is admitted as null when absent" {
    const src =
        \\{"type":"webhook","source":"github"}
    ;
    const trig = try parseTriggerForTest(src);
    defer freeTrigger(trig);
    try std.testing.expect(trig.webhook.events == null);
}

test "parseAgentTrigger.webhook: events of length 1 is admitted" {
    const src =
        \\{"type":"webhook","source":"github","events":["workflow_run"]}
    ;
    const trig = try parseTriggerForTest(src);
    defer freeTrigger(trig);
    const evs = trig.webhook.events.?;
    try std.testing.expectEqual(@as(usize, 1), evs.len);
    try std.testing.expectEqualStrings("workflow_run", evs[0]);
}

test "parseAgentTrigger.webhook: events with mixed entries is admitted" {
    const src =
        \\{"type":"webhook","source":"github","events":["workflow_run","pull_request","issues"]}
    ;
    const trig = try parseTriggerForTest(src);
    defer freeTrigger(trig);
    const evs = trig.webhook.events.?;
    try std.testing.expectEqual(@as(usize, 3), evs.len);
    try std.testing.expectEqualStrings("workflow_run", evs[0]);
    try std.testing.expectEqualStrings("pull_request", evs[1]);
    try std.testing.expectEqualStrings("issues", evs[2]);
}

test "parseAgentTrigger.webhook: empty events array is rejected" {
    const src =
        \\{"type":"webhook","source":"github","events":[]}
    ;
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseTriggerForTest(src));
}

test "parseAgentTrigger.webhook: events array over MAX_EVENTS_PER_TRIGGER is rejected" {
    // 17 entries — one over MAX_EVENTS_PER_TRIGGER (16).
    const src =
        \\{"type":"webhook","source":"github","events":["a","b","c","d","e","f","g","h","i","j","k","l","m","n","o","p","q"]}
    ;
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseTriggerForTest(src));
}

test "parseAgentTrigger.webhook: events entry containing whitespace is rejected" {
    const src =
        \\{"type":"webhook","source":"github","events":["pull request"]}
    ;
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseTriggerForTest(src));
}

test "parseAgentTrigger.webhook: empty events entry is rejected" {
    const src =
        \\{"type":"webhook","source":"github","events":[""]}
    ;
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseTriggerForTest(src));
}

test "parseAgentTrigger.webhook: events entry over MAX_EVENT_NAME_LEN is rejected" {
    // 65 'a's — one over MAX_EVENT_NAME_LEN (64).
    const src =
        \\{"type":"webhook","source":"github","events":["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]}
    ;
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseTriggerForTest(src));
}

test "parseAgentTrigger.webhook: non-string events entry is rejected" {
    const src =
        \\{"type":"webhook","source":"github","events":[42]}
    ;
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseTriggerForTest(src));
}

test "parseAgentTrigger.webhook: non-array events value is rejected" {
    const src =
        \\{"type":"webhook","source":"github","events":"workflow_run"}
    ;
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseTriggerForTest(src));
}

// ── credential_name override ──────────────────────────────────────────────

test "parseAgentTrigger.webhook: credential_name absent defaults to null" {
    const src =
        \\{"type":"webhook","source":"github"}
    ;
    const trig = try parseTriggerForTest(src);
    defer freeTrigger(trig);
    try std.testing.expect(trig.webhook.credential_name == null);
}

test "parseAgentTrigger.webhook: credential_name override is dup'd into the trigger" {
    const src =
        \\{"type":"webhook","source":"github","credential_name":"github-orgA"}
    ;
    const trig = try parseTriggerForTest(src);
    defer freeTrigger(trig);
    try std.testing.expectEqualStrings("github-orgA", trig.webhook.credential_name.?);
}

// ── type: api ─────────────────────────────────────────────────────────────

test "parseAgentTrigger: type=api is admitted" {
    const src =
        \\{"type":"api"}
    ;
    const trig = try parseTriggerForTest(src);
    defer freeTrigger(trig);
    try std.testing.expect(trig == .api);
}

test "parseAgentTrigger: type=foobar is rejected with InvalidTriggerType" {
    const src =
        \\{"type":"foobar"}
    ;
    try std.testing.expectError(AgentConfigError.InvalidTriggerType, parseTriggerForTest(src));
}

// ── parseAgentTriggers: array length + cross-entry rules ─────────────────

test "parseAgentTriggers: single webhook entry is admitted" {
    const trs = try parseTriggersForTest(
        \\[{"type":"webhook","source":"github","events":["workflow_run"]}]
    );
    defer freeTriggers(trs);
    try std.testing.expectEqual(@as(usize, 1), trs.len);
    try std.testing.expectEqualStrings("github", trs[0].webhook.source);
}

test "parseAgentTriggers: empty array is rejected" {
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseTriggersForTest(
        \\[]
    ));
}

test "parseAgentTriggers: array over MAX_TRIGGERS_PER_AGENT is rejected" {
    // 9 entries — one over MAX_TRIGGERS_PER_AGENT (8).
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseTriggersForTest(
        \\[
        \\  {"type":"webhook","source":"a"},
        \\  {"type":"webhook","source":"b"},
        \\  {"type":"webhook","source":"c"},
        \\  {"type":"webhook","source":"d"},
        \\  {"type":"webhook","source":"e"},
        \\  {"type":"webhook","source":"f"},
        \\  {"type":"webhook","source":"g"},
        \\  {"type":"webhook","source":"h"},
        \\  {"type":"webhook","source":"i"}
        \\]
    ));
}

test "parseAgentTriggers: duplicate (type, source) tuple is rejected" {
    try std.testing.expectError(AgentConfigError.InvalidTriggerType, parseTriggersForTest(
        \\[
        \\  {"type":"webhook","source":"github","events":["push"]},
        \\  {"type":"webhook","source":"github","events":["pull_request"]}
        \\]
    ));
}

test "parseAgentTriggers: distinct webhook sources are admitted" {
    const trs = try parseTriggersForTest(
        \\[
        \\  {"type":"webhook","source":"github","events":["workflow_run"]},
        \\  {"type":"webhook","source":"linear"}
        \\]
    );
    defer freeTriggers(trs);
    try std.testing.expectEqual(@as(usize, 2), trs.len);
    try std.testing.expectEqualStrings("github", trs[0].webhook.source);
    try std.testing.expectEqualStrings("linear", trs[1].webhook.source);
}

test "parseAgentTriggers: duplicate api entries are rejected" {
    try std.testing.expectError(AgentConfigError.InvalidTriggerType, parseTriggersForTest(
        \\[
        \\  {"type":"api"},
        \\  {"type":"api"}
        \\]
    ));
}

test "parseAgentTriggers: more than one cron entry is rejected" {
    try std.testing.expectError(AgentConfigError.InvalidTriggerType, parseTriggersForTest(
        \\[
        \\  {"type":"cron","schedule":"0 9 * * *"},
        \\  {"type":"cron","schedule":"0 17 * * *"}
        \\]
    ));
}

test "parseAgentTriggers: webhook plus cron is admitted" {
    const trs = try parseTriggersForTest(
        \\[
        \\  {"type":"webhook","source":"github","events":["workflow_run"]},
        \\  {"type":"cron","schedule":"0 3 * * *"}
        \\]
    );
    defer freeTriggers(trs);
    try std.testing.expectEqual(@as(usize, 2), trs.len);
    try std.testing.expectEqual(config_types.AgentTriggerType.cron, @as(config_types.AgentTriggerType, trs[1]));
    try std.testing.expectEqualStrings("0 3 * * *", trs[1].cron.schedule);
}

test "parseAgentTriggers: webhook plus api is admitted" {
    const trs = try parseTriggersForTest(
        \\[
        \\  {"type":"webhook","source":"github"},
        \\  {"type":"api"}
        \\]
    );
    defer freeTriggers(trs);
    try std.testing.expectEqual(@as(usize, 2), trs.len);
    try std.testing.expectEqual(config_types.AgentTriggerType.api, @as(config_types.AgentTriggerType, trs[1]));
}

test "parseAgentTriggers: non-object array element is rejected" {
    try std.testing.expectError(AgentConfigError.InvalidFieldType, parseTriggersForTest(
        \\[
        \\  {"type":"webhook","source":"github"},
        \\  "not an object"
        \\]
    ));
}
