//! Process-lifetime QStash credentials loaded from the administrative vault.

const Credentials = @This();

const std = @import("std");
const pg = @import("pg");

const secure_memory = @import("../secrets/secure_memory.zig");
const vault = @import("../state/vault.zig");

pub const VAULT_KEY = "qstash";
const TOKEN_FIELD = "token";
const CURRENT_SIGNING_KEY_FIELD = "current_signing_key";
const NEXT_SIGNING_KEY_FIELD = "next_signing_key";
const URL_FIELD = "url";

token: []u8,
current_signing_key: []u8,
next_signing_key: []u8,
// Provider API base (e.g. https://qstash-eu-central-1.upstash.io). Region- and
// environment-specific, so it is sourced from the vault, not hardcoded.
url: []u8,

pub fn load(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    admin_workspace_id: []const u8,
) !Credentials {
    if (admin_workspace_id.len == 0) return error.QStashCredentialInvalid;
    var parsed = try vault.loadJson(alloc, conn, admin_workspace_id, VAULT_KEY);
    defer parsed.deinit();
    return fromObject(alloc, parsed.value.object);
}

pub fn deinit(self: *Credentials, alloc: std.mem.Allocator) void {
    secure_memory.freeBytes(alloc, self.token);
    secure_memory.freeBytes(alloc, self.current_signing_key);
    secure_memory.freeBytes(alloc, self.next_signing_key);
    secure_memory.freeBytes(alloc, self.url);
    self.* = undefined;
}

fn fromObject(alloc: std.mem.Allocator, object: std.json.ObjectMap) !Credentials {
    const token = try dupeField(alloc, object, TOKEN_FIELD);
    errdefer secure_memory.freeBytes(alloc, token);
    const current = try dupeField(alloc, object, CURRENT_SIGNING_KEY_FIELD);
    errdefer secure_memory.freeBytes(alloc, current);
    const next = try dupeField(alloc, object, NEXT_SIGNING_KEY_FIELD);
    errdefer secure_memory.freeBytes(alloc, next);
    const url = try dupeField(alloc, object, URL_FIELD);
    return .{
        .token = token,
        .current_signing_key = current,
        .next_signing_key = next,
        .url = url,
    };
}

fn dupeField(
    alloc: std.mem.Allocator,
    object: std.json.ObjectMap,
    field: []const u8,
) ![]u8 {
    const value = object.get(field) orelse return error.QStashCredentialInvalid;
    if (value != .string or value.string.len == 0) return error.QStashCredentialInvalid;
    return alloc.dupe(u8, value.string);
}

test "fromObject loads url alongside the token and signing keys" {
    const alloc = std.testing.allocator;
    const eu_url = "https://qstash-eu-central-1.upstash.io";
    const json = "{\"token\":\"t\",\"current_signing_key\":\"c\",\"next_signing_key\":\"n\",\"url\":\"" ++ eu_url ++ "\"}";
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, json, .{});
    defer parsed.deinit();
    var creds = try fromObject(alloc, parsed.value.object);
    defer creds.deinit(alloc);
    try std.testing.expectEqualStrings(eu_url, creds.url);
}

test "fromObject rejects a secret bag missing url" {
    const alloc = std.testing.allocator;
    const json = "{\"token\":\"t\",\"current_signing_key\":\"c\",\"next_signing_key\":\"n\"}";
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, json, .{});
    defer parsed.deinit();
    try std.testing.expectError(error.QStashCredentialInvalid, fromObject(alloc, parsed.value.object));
}
