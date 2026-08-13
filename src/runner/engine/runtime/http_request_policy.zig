//! Provider-neutral validation for daemon-authored HTTP origin rules.

const std = @import("std");
const execution_policy = @import("contract").execution_policy;

const HTTPS_SCHEME = "https";
const HTTPS_PORT: u16 = 443;

pub const Verdict = enum { unscoped, allowed, denied };

pub fn validate(
    arena: std.mem.Allocator,
    origins: []const execution_policy.HttpOriginPolicy,
    url: []const u8,
    method_text: []const u8,
    body: ?std.json.Value,
) Verdict {
    const uri = std.Uri.parse(url) catch return .denied;
    const path = uri.path.toRawMaybeAlloc(arena) catch return .denied;
    if (!pathIsSafe(path)) return .denied;
    const host = componentBytes(uri.host orelse return .denied);
    const origin = findOrigin(origins, host) orelse return .unscoped;
    if (!std.ascii.eqlIgnoreCase(uri.scheme, HTTPS_SCHEME)) return .denied;
    if (uri.port) |port| if (port != HTTPS_PORT) return .denied;
    const method = parseMethod(method_text) orelse return .denied;
    for (origin.requests) |rule| {
        if (rule.method != method or !pathAllowed(rule, path)) continue;
        if (jsonFieldsAllowed(arena, rule.json_fields, body)) return .allowed;
    }
    return .denied;
}

/// Reject traversal spellings before prefix matching. The decoded path is the
/// same spelling the rule matcher sees, so an HTTP client cannot normalize a
/// request into a path broader than the one the runner authorized.
fn pathIsSafe(path: []const u8) bool {
    if (std.mem.indexOfScalar(u8, path, '\\') != null) return false;
    if (containsEncodedPathByte(path)) return false;
    var segments = std.mem.splitScalar(u8, path, '/');
    while (segments.next()) |segment| {
        if (std.mem.eql(u8, segment, ".") or std.mem.eql(u8, segment, "..")) return false;
    }
    return true;
}

/// A percent sign that remains after URI decoding is a second encoding layer.
/// Reject encoded dot and separator bytes so neither a client nor the remote
/// server can reveal traversal syntax after this policy check.
fn containsEncodedPathByte(path: []const u8) bool {
    var i: usize = 0;
    while (i + 2 < path.len) : (i += 1) {
        if (path[i] != '%') continue;
        const high = std.ascii.toLower(path[i + 1]);
        const low = std.ascii.toLower(path[i + 2]);
        if ((high == '2' and (low == 'e' or low == 'f')) or
            (high == '5' and low == 'c')) return true;
    }
    return false;
}

pub fn credentialAllowed(
    origins: []const execution_policy.HttpOriginPolicy,
    credential_name: []const u8,
    host: []const u8,
) bool {
    const origin = findOrigin(origins, host) orelse return false;
    for (origin.credential_names) |allowed| {
        if (std.mem.eql(u8, allowed, credential_name)) return true;
    }
    return false;
}

fn findOrigin(
    origins: []const execution_policy.HttpOriginPolicy,
    host: []const u8,
) ?execution_policy.HttpOriginPolicy {
    for (origins) |origin| {
        if (std.ascii.eqlIgnoreCase(origin.host, host)) return origin;
    }
    return null;
}

fn parseMethod(text: []const u8) ?execution_policy.HttpMethod {
    if (std.ascii.eqlIgnoreCase(text, "GET")) return .get;
    if (std.ascii.eqlIgnoreCase(text, "HEAD")) return .head;
    if (std.ascii.eqlIgnoreCase(text, "POST")) return .post;
    return null;
}

fn pathAllowed(rule: execution_policy.HttpRequestRule, path: []const u8) bool {
    return switch (rule.path_match) {
        .exact => std.mem.eql(u8, path, rule.path),
        .prefix => std.mem.startsWith(u8, path, rule.path),
    };
}

fn jsonFieldsAllowed(
    arena: std.mem.Allocator,
    rules: []const execution_policy.HttpJsonFieldRule,
    body: ?std.json.Value,
) bool {
    if (rules.len == 0) return true;
    const object = bodyObject(arena, body) orelse return false;
    for (rules) |rule| {
        const value = object.get(rule.name) orelse return false;
        const expects_string = rule.string_value != null;
        const expects_boolean = rule.boolean_value != null;
        if (expects_string == expects_boolean) return false;
        if (rule.string_value) |expected| {
            if (value != .string or !std.mem.eql(u8, value.string, expected)) return false;
        } else if (rule.boolean_value) |expected| {
            if (value != .bool or value.bool != expected) return false;
        }
    }
    return true;
}

fn bodyObject(arena: std.mem.Allocator, body: ?std.json.Value) ?std.json.ObjectMap {
    const value = body orelse return null;
    if (value == .object) return value.object;
    if (value != .string) return null;
    const parsed = std.json.parseFromSliceLeaky(std.json.Value, arena, value.string, .{}) catch return null;
    return if (parsed == .object) parsed.object else null;
}

fn componentBytes(component: std.Uri.Component) []const u8 {
    return switch (component) {
        .raw, .percent_encoded => |bytes| bytes,
    };
}
