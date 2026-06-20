// Fleet config sub-parsers: trigger, network, budget, skill validation.
//
// Extracted from config.zig to keep files under 400 lines.
// These are pure parse functions operating on std.json.ObjectMap.

const std = @import("std");
const Allocator = std.mem.Allocator;
const logging = @import("log");

const config_types = @import("config_types.zig");
const webhook_verify = @import("webhook_verify.zig");
const MS_PER_SECOND = 1000.0;
const MAX_BUDGET_UNITS = 10000.0;

const log = logging.scoped(.fleet_config);
const FleetTrigger = config_types.FleetTrigger;
const FleetNetwork = config_types.FleetNetwork;
const FleetBudget = config_types.FleetBudget;
const FleetConfigError = config_types.FleetConfigError;
const WebhookSignatureConfig = config_types.WebhookSignatureConfig;
const MAX_SIGNATURE_HEADER_LEN = config_types.MAX_SIGNATURE_HEADER_LEN;

// Trigger array + event allow-list bounds. Local to the parser so the
// type module stays free of validation knobs; tests reach in via the
// parser's behaviour, not the constants. RULE NSQ: every limit is a
// named constant — no inline `8`/`16`/`64` at validation sites.
pub const MAX_TRIGGERS_PER_AGENT: usize = 8;
const MAX_EVENTS_PER_TRIGGER: usize = 16;
const MAX_EVENT_NAME_LEN: usize = 64;

/// Parse a `triggers[]` array element.
pub fn parseFleetTrigger(alloc: Allocator, obj: std.json.ObjectMap) (Allocator.Error || FleetConfigError)!FleetTrigger {
    const type_str = blk: {
        const val = obj.get("type") orelse return FleetConfigError.MissingRequiredField;
        break :blk switch (val) {
            .string => |s| s,
            else => return FleetConfigError.MissingRequiredField,
        };
    };

    if (std.mem.eql(u8, type_str, "webhook")) {
        const source = try requireString(alloc, obj, "source") orelse return FleetConfigError.InvalidTriggerSource;
        errdefer alloc.free(source);
        const events = try parseEvents(alloc, obj);
        errdefer if (events) |evs| {
            for (evs) |e| alloc.free(e);
            alloc.free(evs);
        };
        const credential_name = try optionalString(alloc, obj, "credential_name");
        errdefer if (credential_name) |c| alloc.free(c);
        const signature = try parseWebhookSignature(alloc, obj, source);
        return .{ .webhook = .{
            .source = source,
            .events = events,
            .credential_name = credential_name,
            .signature = signature,
        } };
    }
    if (std.mem.eql(u8, type_str, "cron")) {
        const schedule = try requireString(alloc, obj, "schedule") orelse return FleetConfigError.MissingRequiredField;
        return .{ .cron = .{ .schedule = schedule } };
    }
    if (std.mem.eql(u8, type_str, "api")) {
        return .{ .api = {} };
    }
    return FleetConfigError.InvalidTriggerType;
}

/// Parse the `triggers:` array under `x-agentsfleet:`. Enforces:
///   * length in 1..MAX_TRIGGERS_PER_AGENT
///   * at most one cron entry
///   * unique `(type, source)` tuple across webhook entries
/// On error every successfully-parsed trigger is freed before propagating.
pub fn parseFleetTriggers(
    alloc: Allocator,
    items: []const std.json.Value,
) (Allocator.Error || FleetConfigError)![]const FleetTrigger {
    if (items.len == 0 or items.len > MAX_TRIGGERS_PER_AGENT) {
        log.warn("triggers_count_out_of_bounds", .{ .count = items.len, .max = MAX_TRIGGERS_PER_AGENT });
        return FleetConfigError.InvalidFieldType;
    }
    var out = try alloc.alloc(FleetTrigger, items.len);
    var parsed_count: usize = 0;
    errdefer {
        for (out[0..parsed_count]) |t| config_types.freeFleetTrigger(alloc, t);
        alloc.free(out);
    }
    var cron_count: usize = 0;
    for (items, 0..) |item, idx| {
        const obj = switch (item) {
            .object => |o| o,
            else => return FleetConfigError.InvalidFieldType,
        };
        const trig = try parseFleetTrigger(alloc, obj);
        out[idx] = trig;
        parsed_count += 1;
        if (trig == .cron) {
            cron_count += 1;
            if (cron_count > 1) {
                log.warn("multiple_cron_triggers_rejected", .{});
                return FleetConfigError.InvalidTriggerType;
            }
        }
        for (out[0..idx]) |existing| {
            if (std.meta.activeTag(existing) != std.meta.activeTag(trig)) continue;
            const conflict = switch (trig) {
                .webhook => |w| std.mem.eql(u8, existing.webhook.source, w.source),
                .cron => false, // ≤1-cron rule handles this above
                .api => true,
            };
            if (conflict) {
                log.warn("duplicate_trigger_tuple", .{ .index = idx });
                return FleetConfigError.InvalidTriggerType;
            }
        }
    }
    return out;
}

fn parseEvents(
    alloc: Allocator,
    obj: std.json.ObjectMap,
) (Allocator.Error || FleetConfigError)!?[]const []const u8 {
    const val = obj.get("events") orelse return null;
    const arr = switch (val) {
        .array => |a| a,
        else => {
            log.warn("events_must_be_array", .{});
            return FleetConfigError.InvalidFieldType;
        },
    };
    if (arr.items.len == 0 or arr.items.len > MAX_EVENTS_PER_TRIGGER) {
        log.warn("events_count_out_of_bounds", .{ .count = arr.items.len, .max = MAX_EVENTS_PER_TRIGGER });
        return FleetConfigError.InvalidFieldType;
    }
    var out = try alloc.alloc([]const u8, arr.items.len);
    var i: usize = 0;
    errdefer {
        for (out[0..i]) |s| alloc.free(s);
        alloc.free(out);
    }
    for (arr.items) |item| {
        const s = switch (item) {
            .string => |str| str,
            else => {
                log.warn("events_entry_not_string", .{});
                return FleetConfigError.InvalidFieldType;
            },
        };
        if (s.len == 0 or s.len > MAX_EVENT_NAME_LEN) {
            log.warn("events_entry_length_out_of_bounds", .{ .len = s.len, .max = MAX_EVENT_NAME_LEN });
            return FleetConfigError.InvalidFieldType;
        }
        for (s) |c| {
            if (std.ascii.isWhitespace(c)) {
                log.warn("events_entry_has_whitespace", .{});
                return FleetConfigError.InvalidFieldType;
            }
        }
        out[i] = try alloc.dupe(u8, s);
        i += 1;
    }
    return out;
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
        return FleetConfigError.InvalidSignatureConfig;
    errdefer alloc.free(secret_ref);
    if (secret_ref.len == 0) return FleetConfigError.InvalidSignatureConfig;

    const registry_hit = webhook_verify.detectProvider(source, webhook_verify.NoHeaders{});

    const header = header_blk: {
        if (try optionalString(alloc, sig_obj, "header")) |h| break :header_blk h;
        if (registry_hit) |cfg| break :header_blk try alloc.dupe(u8, cfg.sig_header);
        return FleetConfigError.InvalidSignatureConfig;
    };
    errdefer alloc.free(header);
    if (header.len > MAX_SIGNATURE_HEADER_LEN) return FleetConfigError.InvalidSignatureConfig;

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

pub fn parseFleetNetwork(alloc: Allocator, obj: std.json.ObjectMap) (Allocator.Error || FleetConfigError)!FleetNetwork {
    const allow_val = obj.get("allow") orelse return FleetNetwork{ .allow = &.{} };
    const allow_arr = switch (allow_val) {
        .array => |a| a,
        else => return FleetConfigError.MissingRequiredField,
    };
    return FleetNetwork{ .allow = try dupeStringArray(alloc, allow_arr.items) };
}

pub fn parseFleetBudget(obj: std.json.ObjectMap) FleetConfigError!FleetBudget {
    const daily_val = obj.get("daily_dollars") orelse return FleetConfigError.MissingRequiredField;
    const daily = switch (daily_val) {
        .float => |f| f,
        .integer => |i| @as(f64, @floatFromInt(i)),
        else => return FleetConfigError.InvalidBudget,
    };
    if (daily <= 0.0 or daily > MS_PER_SECOND) return FleetConfigError.InvalidBudget;

    const monthly: ?f64 = blk: {
        const val = obj.get("monthly_dollars") orelse break :blk null;
        const f: f64 = switch (val) {
            .float => |fv| fv,
            .integer => |i| @as(f64, @floatFromInt(i)),
            else => return FleetConfigError.InvalidBudget,
        };
        if (f <= 0.0 or f > MAX_BUDGET_UNITS) return FleetConfigError.InvalidBudget;
        break :blk f;
    };

    return FleetBudget{ .daily_dollars = daily, .monthly_dollars = monthly };
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
            else => return FleetConfigError.MissingRequiredField,
        };
        out[i] = try alloc.dupe(u8, s);
        i += 1;
    }
    return out;
}

// Tests live in `config_helpers_test.zig` to keep this implementation
// file under the 350-line cap and so the validation matrix
// (parseWebhookSignature + parseEvents + parseFleetTriggers) lives in
// one place.
