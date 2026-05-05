// Zombie config sub-parsers: trigger, network, budget, skill validation.
//
// Extracted from config.zig to keep files under 400 lines.
// These are pure parse functions operating on std.json.ObjectMap.

const std = @import("std");
const Allocator = std.mem.Allocator;

const config_types = @import("config_types.zig");
const webhook_verify = @import("webhook_verify.zig");
const ZombieTriggerType = config_types.ZombieTriggerType;
const ZombieTrigger = config_types.ZombieTrigger;
const ZombieNetwork = config_types.ZombieNetwork;
const ZombieBudget = config_types.ZombieBudget;
const ZombieConfigError = config_types.ZombieConfigError;
const WebhookSignatureConfig = config_types.WebhookSignatureConfig;
const MAX_SIGNATURE_HEADER_LEN = config_types.MAX_SIGNATURE_HEADER_LEN;

pub fn parseZombieTrigger(alloc: Allocator, obj: std.json.ObjectMap) (Allocator.Error || ZombieConfigError)!ZombieTrigger {
    const type_str = blk: {
        const val = obj.get("type") orelse return ZombieConfigError.MissingRequiredField;
        break :blk switch (val) {
            .string => |s| s,
            else => return ZombieConfigError.MissingRequiredField,
        };
    };

    if (std.mem.eql(u8, type_str, "webhook")) {
        const source = try requireString(alloc, obj, "source") orelse return ZombieConfigError.InvalidTriggerSource;
        errdefer alloc.free(source);
        const event = try optionalString(alloc, obj, "event");
        errdefer if (event) |e| alloc.free(e);
        const signature = try parseWebhookSignature(alloc, obj, source);
        return .{ .webhook = .{ .source = source, .event = event, .signature = signature } };
    }
    if (std.mem.eql(u8, type_str, "cron")) {
        const schedule = try requireString(alloc, obj, "schedule") orelse return ZombieConfigError.MissingRequiredField;
        return .{ .cron = .{ .schedule = schedule } };
    }
    if (std.mem.eql(u8, type_str, "api")) return .{ .api = {} };
    return ZombieConfigError.InvalidTriggerType;
}

fn parseWebhookSignature(
    alloc: Allocator,
    obj: std.json.ObjectMap,
    source: []const u8,
) !?WebhookSignatureConfig {
    const sig_val = obj.get("signature") orelse return null;
    const sig_obj = switch (sig_val) {
        .object => |o| o,
        else => return null,
    };

    const secret_ref = try requireString(alloc, sig_obj, "secret_ref") orelse
        return ZombieConfigError.InvalidSignatureConfig;
    errdefer alloc.free(secret_ref);
    if (secret_ref.len == 0) return ZombieConfigError.InvalidSignatureConfig;

    const registry_hit = webhook_verify.detectProvider(source, webhook_verify.NoHeaders{});

    const header = header_blk: {
        if (try optionalString(alloc, sig_obj, "header")) |h| break :header_blk h;
        if (registry_hit) |cfg| break :header_blk try alloc.dupe(u8, cfg.sig_header);
        return ZombieConfigError.InvalidSignatureConfig;
    };
    errdefer alloc.free(header);
    if (header.len > MAX_SIGNATURE_HEADER_LEN) return ZombieConfigError.InvalidSignatureConfig;

    const prefix = prefix_blk: {
        if (try optionalString(alloc, sig_obj, "prefix")) |p| break :prefix_blk p;
        if (registry_hit) |cfg| break :prefix_blk try alloc.dupe(u8, cfg.prefix);
        break :prefix_blk try alloc.dupe(u8, "");
    };
    errdefer alloc.free(prefix);

    const ts_header = ts_blk: {
        if (try optionalString(alloc, sig_obj, "ts_header")) |t| break :ts_blk t;
        if (registry_hit) |cfg| {
            if (cfg.ts_header) |t| break :ts_blk try alloc.dupe(u8, t);
        }
        break :ts_blk null;
    };

    return .{
        .header = header,
        .prefix = prefix,
        .ts_header = ts_header,
        .secret_ref = secret_ref,
    };
}

fn requireString(alloc: Allocator, obj: std.json.ObjectMap, key: []const u8) !?[]const u8 {
    const val = obj.get(key) orelse return null;
    const s = switch (val) {
        .string => |str| str,
        else => return null,
    };
    return try alloc.dupe(u8, s);
}

fn optionalString(alloc: Allocator, obj: std.json.ObjectMap, key: []const u8) !?[]const u8 {
    const val = obj.get(key) orelse return null;
    const s = switch (val) {
        .string => |str| str,
        else => return null,
    };
    return try alloc.dupe(u8, s);
}

pub fn parseZombieNetwork(alloc: Allocator, obj: std.json.ObjectMap) (Allocator.Error || ZombieConfigError)!ZombieNetwork {
    const allow_val = obj.get("allow") orelse return ZombieNetwork{ .allow = &.{} };
    const allow_arr = switch (allow_val) {
        .array => |a| a,
        else => return ZombieConfigError.MissingRequiredField,
    };
    return ZombieNetwork{ .allow = try dupeStringArray(alloc, allow_arr.items) };
}

pub fn parseZombieBudget(obj: std.json.ObjectMap) ZombieConfigError!ZombieBudget {
    const daily_val = obj.get("daily_dollars") orelse return ZombieConfigError.MissingRequiredField;
    const daily = switch (daily_val) {
        .float => |f| f,
        .integer => |i| @as(f64, @floatFromInt(i)),
        else => return ZombieConfigError.InvalidBudget,
    };
    if (daily <= 0.0 or daily > 1000.0) return ZombieConfigError.InvalidBudget;

    const monthly: ?f64 = blk: {
        const val = obj.get("monthly_dollars") orelse break :blk null;
        const f: f64 = switch (val) {
            .float => |fv| fv,
            .integer => |i| @as(f64, @floatFromInt(i)),
            else => return ZombieConfigError.InvalidBudget,
        };
        if (f <= 0.0 or f > 10000.0) return ZombieConfigError.InvalidBudget;
        break :blk f;
    };

    return ZombieBudget{ .daily_dollars = daily, .monthly_dollars = monthly };
}

pub fn dupeStringArray(alloc: Allocator, items: []const std.json.Value) ![]const []const u8 {
    const out = try alloc.alloc([]const u8, items.len);
    var i: usize = 0;
    errdefer {
        for (out[0..i]) |s| alloc.free(s);
        alloc.free(out);
    }
    for (items) |item| {
        const s = switch (item) {
            .string => |str| str,
            else => return ZombieConfigError.MissingRequiredField,
        };
        out[i] = try alloc.dupe(u8, s);
        i += 1;
    }
    return out;
}

// ── parseWebhookSignature (§3 + §4.7) ────────────────────────────────────
// Exercised via parseZombieTrigger (pub). Uses std.json.parseFromSlice to
// produce the ObjectMap.

const test_alloc = std.testing.allocator;

fn parseTriggerForTest(src: []const u8) !ZombieTrigger {
    const parsed = try std.json.parseFromSlice(std.json.Value, test_alloc, src, .{});
    defer parsed.deinit();
    return parseZombieTrigger(test_alloc, parsed.value.object);
}

fn freeTrigger(t: ZombieTrigger) void {
    config_types.freeZombieTrigger(test_alloc, t);
}

test "parseWebhookSignature: defaults from github registry (dim 3.1)" {
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

test "parseWebhookSignature: defaults from linear registry (dim 3.7, Q1)" {
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

test "parseWebhookSignature: no signature block = null (backward compat, dim 3.2)" {
    const src =
        \\{"type":"webhook","source":"agentmail"}
    ;
    const trig = try parseTriggerForTest(src);
    defer freeTrigger(trig);
    try std.testing.expect(trig.webhook.signature == null);
}

test "parseWebhookSignature: Jira custom scheme (not in registry)" {
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

test "parseWebhookSignature: explicit header overrides registry (escape hatch)" {
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
    try std.testing.expectError(ZombieConfigError.InvalidSignatureConfig, parseTriggerForTest(src));
}

test "parseWebhookSignature: empty secret_ref is rejected" {
    const src =
        \\{"type":"webhook","source":"github","signature":{"secret_ref":""}}
    ;
    try std.testing.expectError(ZombieConfigError.InvalidSignatureConfig, parseTriggerForTest(src));
}

test "parseWebhookSignature: unknown source without header is rejected" {
    const src =
        \\{"type":"webhook","source":"custom_provider","signature":{"secret_ref":"s"}}
    ;
    try std.testing.expectError(ZombieConfigError.InvalidSignatureConfig, parseTriggerForTest(src));
}

test "parseWebhookSignature: header > 64 chars is rejected" {
    // 65 'a's
    const src =
        \\{"type":"webhook","source":"jira","signature":{"secret_ref":"s","header":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}
    ;
    try std.testing.expectError(ZombieConfigError.InvalidSignatureConfig, parseTriggerForTest(src));
}
