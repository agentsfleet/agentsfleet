const std = @import("std");
const cb = @import("clerk_backend.zig");

test "renderMetadataPayload: both fields → compact merge body" {
    const alloc = std.testing.allocator;
    const payload = try cb.renderMetadataPayload(alloc, "0195b4ba-8d3a-7f13-8abc-aa0000000001", "operator");
    defer alloc.free(payload);
    try std.testing.expectEqualStrings(
        \\{"public_metadata":{"tenant_id":"0195b4ba-8d3a-7f13-8abc-aa0000000001","role":"operator"}}
    , payload);
}

test "renderMetadataPayload: tenant_id only" {
    const alloc = std.testing.allocator;
    const payload = try cb.renderMetadataPayload(alloc, "t_abc", null);
    defer alloc.free(payload);
    try std.testing.expectEqualStrings(
        \\{"public_metadata":{"tenant_id":"t_abc"}}
    , payload);
}

test "renderMetadataPayload: role only" {
    const alloc = std.testing.allocator;
    const payload = try cb.renderMetadataPayload(alloc, null, "admin");
    defer alloc.free(payload);
    try std.testing.expectEqualStrings(
        \\{"public_metadata":{"role":"admin"}}
    , payload);
}

test "renderMetadataPayload: escapes JSON-unsafe chars in values" {
    const alloc = std.testing.allocator;
    const payload = try cb.renderMetadataPayload(alloc, "quoted\"name", "oper\\ator");
    defer alloc.free(payload);
    // Both fields route through `writeJsonEscaped` — backslash + quote
    // must be escaped, preserving the key ordering we rely on.
    try std.testing.expect(std.mem.indexOf(u8, payload, "\\\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, payload, "\\\\") != null);
}

test "renderMetadataPayload: both null → empty metadata object" {
    const alloc = std.testing.allocator;
    const payload = try cb.renderMetadataPayload(alloc, null, null);
    defer alloc.free(payload);
    try std.testing.expectEqualStrings(
        \\{"public_metadata":{}}
    , payload);
}

test "mapStatus: 2xx returns success" {
    try cb.mapStatus(200, "https://api.clerk.com/v1/users/u/metadata");
    try cb.mapStatus(201, "https://api.clerk.com/v1/users/u/metadata");
    try cb.mapStatus(299, "https://api.clerk.com/v1/users/u/metadata");
}

test "mapStatus: 401 + 403 map to Unauthorized" {
    try std.testing.expectError(cb.PatchError.Unauthorized, cb.mapStatus(401, "x"));
    try std.testing.expectError(cb.PatchError.Unauthorized, cb.mapStatus(403, "x"));
}

test "mapStatus: 404 maps to NotFound" {
    try std.testing.expectError(cb.PatchError.NotFound, cb.mapStatus(404, "x"));
}

test "mapStatus: anything else maps to UnexpectedStatus" {
    try std.testing.expectError(cb.PatchError.UnexpectedStatus, cb.mapStatus(400, "x"));
    try std.testing.expectError(cb.PatchError.UnexpectedStatus, cb.mapStatus(429, "x"));
    try std.testing.expectError(cb.PatchError.UnexpectedStatus, cb.mapStatus(500, "x"));
    try std.testing.expectError(cb.PatchError.UnexpectedStatus, cb.mapStatus(503, "x"));
}

test "patchUserPublicMetadata: missing CLERK_SECRET_KEY returns MissingSecret" {
    // Nothing to unset in the test process — if the env var happens to be
    // populated on the runner, skip rather than risk an outbound call.
    if (std.process.getEnvVarOwned(std.testing.allocator, cb.SECRET_ENV_VAR)) |v| {
        std.testing.allocator.free(v);
        return error.SkipZigTest;
    } else |_| {}

    try std.testing.expectError(
        cb.PatchError.MissingSecret,
        cb.patchUserPublicMetadata(std.testing.allocator, "user_test", "t_abc", "operator"),
    );
}
