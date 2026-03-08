const std = @import("std");
const pg = @import("pg");
const secrets = @import("../../secrets/crypto.zig");

pub const Route = struct {
    workspace_id: []const u8,
    skill_ref_encoded: []const u8,
    key_name_encoded: []const u8,
};

pub const PutInput = struct {
    value: []const u8,
    scope: ?[]const u8 = null,
    meta_json: ?[]const u8 = null,
};

pub const PutOutput = struct {
    skill_ref: []u8,
    key_name: []u8,
    scope: secrets.SkillSecretScope,
};

pub const DeleteOutput = struct {
    skill_ref: []u8,
    key_name: []u8,
};

pub const SkillSecretError = error{
    InvalidRequest,
};

pub fn parseRoute(path: []const u8) ?Route {
    const prefix = "/v1/workspaces/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const rem = path[prefix.len..];

    const s1 = std.mem.indexOfScalar(u8, rem, '/') orelse return null;
    const workspace_id = rem[0..s1];
    if (workspace_id.len == 0) return null;
    const rem2 = rem[s1 + 1 ..];
    const skills_prefix = "skills/";
    if (!std.mem.startsWith(u8, rem2, skills_prefix)) return null;
    const rem3 = rem2[skills_prefix.len..];
    const s2 = std.mem.indexOfScalar(u8, rem3, '/') orelse return null;
    const skill_ref = rem3[0..s2];
    if (skill_ref.len == 0) return null;
    const rem4 = rem3[s2 + 1 ..];
    const secrets_prefix = "secrets/";
    if (!std.mem.startsWith(u8, rem4, secrets_prefix)) return null;
    const key_name = rem4[secrets_prefix.len..];
    if (key_name.len == 0 or std.mem.indexOfScalar(u8, key_name, '/') != null) return null;
    return .{
        .workspace_id = workspace_id,
        .skill_ref_encoded = skill_ref,
        .key_name_encoded = key_name,
    };
}

pub fn put(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    skill_ref_encoded: []const u8,
    key_name_encoded: []const u8,
    input: PutInput,
) (SkillSecretError || anyerror)!PutOutput {
    if (input.value.len == 0) return SkillSecretError.InvalidRequest;

    const skill_ref = try decodePathSegment(alloc, skill_ref_encoded);
    errdefer alloc.free(skill_ref);
    const key_name = try decodePathSegment(alloc, key_name_encoded);
    errdefer alloc.free(key_name);
    if (key_name.len == 0 or std.mem.indexOfAny(u8, key_name, " \t\r\n") != null) {
        return SkillSecretError.InvalidRequest;
    }

    const scope = if (input.scope) |raw| blk: {
        if (std.ascii.eqlIgnoreCase(raw, "host")) break :blk secrets.SkillSecretScope.host;
        if (std.ascii.eqlIgnoreCase(raw, "sandbox")) break :blk secrets.SkillSecretScope.sandbox;
        return SkillSecretError.InvalidRequest;
    } else secrets.SkillSecretScope.sandbox;

    const kek = try secrets.loadKek(alloc);
    try secrets.storeWorkspaceSkillSecret(
        alloc,
        conn,
        workspace_id,
        skill_ref,
        key_name,
        input.value,
        scope,
        input.meta_json orelse "{}",
        kek,
    );

    return .{
        .skill_ref = skill_ref,
        .key_name = key_name,
        .scope = scope,
    };
}

pub fn delete(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    skill_ref_encoded: []const u8,
    key_name_encoded: []const u8,
) !DeleteOutput {
    const skill_ref = try decodePathSegment(alloc, skill_ref_encoded);
    errdefer alloc.free(skill_ref);
    const key_name = try decodePathSegment(alloc, key_name_encoded);
    errdefer alloc.free(key_name);

    try secrets.deleteWorkspaceSkillSecret(conn, workspace_id, skill_ref, key_name);
    return .{
        .skill_ref = skill_ref,
        .key_name = key_name,
    };
}

pub fn decodePathSegment(alloc: std.mem.Allocator, value: []const u8) ![]u8 {
    var out = std.ArrayList(u8).empty;
    defer out.deinit(alloc);

    var i: usize = 0;
    while (i < value.len) : (i += 1) {
        if (value[i] == '%' and i + 2 < value.len) {
            const hi = std.fmt.charToDigit(value[i + 1], 16) catch return error.InvalidPercentEncoding;
            const lo = std.fmt.charToDigit(value[i + 2], 16) catch return error.InvalidPercentEncoding;
            try out.append(alloc, @as(u8, @intCast(hi * 16 + lo)));
            i += 2;
            continue;
        }
        if (value[i] == '+') {
            try out.append(alloc, ' ');
            continue;
        }
        try out.append(alloc, value[i]);
    }
    return out.toOwnedSlice(alloc);
}

test "parseRoute extracts workspace, skill_ref, and key_name" {
    const route = parseRoute("/v1/workspaces/ws_123/skills/clawhub%3A%2F%2Fopenclaw%2Freviewer%401.2.0/secrets/API_KEY") orelse
        return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("ws_123", route.workspace_id);
    try std.testing.expectEqualStrings("clawhub%3A%2F%2Fopenclaw%2Freviewer%401.2.0", route.skill_ref_encoded);
    try std.testing.expectEqualStrings("API_KEY", route.key_name_encoded);
}

test "decodePathSegment decodes percent-encoded path segments" {
    const decoded = try decodePathSegment(std.testing.allocator, "clawhub%3A%2F%2Fopenclaw%2Freviewer%401.2.0");
    defer std.testing.allocator.free(decoded);
    try std.testing.expectEqualStrings("clawhub://openclaw/reviewer@1.2.0", decoded);
}
