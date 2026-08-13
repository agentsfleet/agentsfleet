//! Credential placement and host-binding rules for outbound HTTP requests.

const std = @import("std");
const nullclaw = @import("nullclaw");
const JsonObjectMap = nullclaw.tools.JsonObjectMap;
const execution_policy = @import("contract").execution_policy;
const http_request_policy = @import("http_request_policy.zig");
const request_args = @import("request_args.zig");

const FIELD_HOST = "host";
const HEADER_AUTHORIZATION = "Authorization";
const SECRET_PREFIX = "${secrets.";
const SECRET_NAME_PREFIX_FMT = "{s}{s}.";
const SECRET_FIELD_FMT = "{s}{s}.{s}}}";

pub fn requestAllowed(policy: *const execution_policy.ExecutionPolicy, args: JsonObjectMap) bool {
    if (!policy.network_policy.read_only and policy.http_origin_policies.len == 0) return true;
    if (valueContainsSecret(args.get(request_args.ARG_BODY), SECRET_PREFIX)) return false;
    const url = stringValue(args.get(request_args.ARG_URL)) orelse return false;
    if (staticCredentialValueInUrl(policy, url)) return false;

    const headers = args.get(request_args.ARG_HEADERS) orelse return true;
    if (headers != .object) return true;
    var it = headers.object.iterator();
    while (it.next()) |entry| {
        if (!valueContainsSecret(entry.value_ptr.*, SECRET_PREFIX)) continue;
        if (!std.ascii.eqlIgnoreCase(entry.key_ptr.*, HEADER_AUTHORIZATION)) return false;
    }
    return true;
}

pub fn credentialsBoundToHost(
    policy: *const execution_policy.ExecutionPolicy,
    host: []const u8,
    args: JsonObjectMap,
) bool {
    if (!policy.network_policy.read_only and policy.http_origin_policies.len == 0) return true;
    if (policy.secrets_map) |secrets| {
        if (secrets == .object) {
            var it = secrets.object.iterator();
            while (it.next()) |entry| {
                if (credentialUsed(entry.key_ptr.*, args) and
                    !staticCredentialMatchesHost(entry.value_ptr.*, host)) return false;
            }
        }
    }
    for (policy.mintable) |mintable| {
        if (credentialUsed(mintable.name, args) and
            !http_request_policy.credentialAllowed(policy.http_origin_policies, mintable.name, host)) return false;
    }
    return true;
}

pub fn mintableCredentialInUrl(policy: *const execution_policy.ExecutionPolicy, url: []const u8) bool {
    for (policy.mintable) |mintable| {
        var needle_buf: [128]u8 = undefined;
        const needle = std.fmt.bufPrint(
            &needle_buf,
            SECRET_NAME_PREFIX_FMT,
            .{ SECRET_PREFIX, mintable.name },
        ) catch return true;
        if (std.mem.indexOf(u8, url, needle) != null) return true;
    }
    return false;
}

fn staticCredentialValueInUrl(policy: *const execution_policy.ExecutionPolicy, url: []const u8) bool {
    const secrets = policy.secrets_map orelse return false;
    if (secrets != .object) return true;
    var credentials = secrets.object.iterator();
    while (credentials.next()) |credential| {
        if (credential.value_ptr.* != .object) return true;
        var fields = credential.value_ptr.object.iterator();
        while (fields.next()) |field| {
            if (std.mem.eql(u8, field.key_ptr.*, FIELD_HOST)) continue;
            var needle_buf: [256]u8 = undefined;
            const needle = std.fmt.bufPrint(
                &needle_buf,
                SECRET_FIELD_FMT,
                .{ SECRET_PREFIX, credential.key_ptr.*, field.key_ptr.* },
            ) catch return true;
            if (std.mem.indexOf(u8, url, needle) != null) return true;
        }
    }
    return false;
}

fn credentialUsed(name: []const u8, args: JsonObjectMap) bool {
    var needle_buf: [128]u8 = undefined;
    const needle = std.fmt.bufPrint(
        &needle_buf,
        SECRET_NAME_PREFIX_FMT,
        .{ SECRET_PREFIX, name },
    ) catch return true;
    if (valueContainsSecret(args.get(request_args.ARG_URL), needle) or
        valueContainsSecret(args.get(request_args.ARG_BODY), needle)) return true;
    const headers = args.get(request_args.ARG_HEADERS) orelse return false;
    if (headers != .object) return false;
    var it = headers.object.iterator();
    while (it.next()) |entry| if (valueContainsSecret(entry.value_ptr.*, needle)) return true;
    return false;
}

fn valueContainsSecret(value: ?std.json.Value, needle: []const u8) bool {
    const field = value orelse return false;
    return switch (field) {
        .string => |text| std.mem.indexOf(u8, text, needle) != null,
        .array => |items| blk: {
            for (items.items) |item| {
                if (valueContainsSecret(item, needle)) break :blk true;
            }
            break :blk false;
        },
        .object => |object| blk: {
            var fields = object.iterator();
            while (fields.next()) |entry| {
                if (valueContainsSecret(entry.value_ptr.*, needle)) break :blk true;
            }
            break :blk false;
        },
        else => false,
    };
}

fn stringValue(value: ?std.json.Value) ?[]const u8 {
    const field = value orelse return null;
    return if (field == .string) field.string else null;
}

fn staticCredentialMatchesHost(value: std.json.Value, host: []const u8) bool {
    const fields = switch (value) {
        .object => |object| object,
        else => return false,
    };
    const credential_host = fields.get(FIELD_HOST) orelse return false;
    return switch (credential_host) {
        .string => |configured| std.ascii.eqlIgnoreCase(configured, host),
        else => false,
    };
}
