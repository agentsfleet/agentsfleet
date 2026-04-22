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

test "renderMetadataPayload: security — control chars + DEL + embedded NUL escape to \\uXXXX" {
    const alloc = std.testing.allocator;
    // NUL, BEL, BS, VT, FF, SI, DEL — every control byte that could
    // otherwise smuggle a control character through into Clerk's JSON
    // parser or a downstream log pipeline. Also prove a literal
    // newline (0x0A) routes through the \n branch.
    const nasty = "\x00\x07\x08\x0b\x0c\x0f\x7f\n";
    const payload = try cb.renderMetadataPayload(alloc, nasty, "operator");
    defer alloc.free(payload);

    // All NUL/BEL/BS/VT/FF/SI/DEL bytes must be hex-escaped; the literal
    // newline must appear as \n. No raw control byte may survive in the
    // output — if one does, it means the escape table missed a branch
    // and an attacker-controlled tenant_id could inject log noise or a
    // fake record separator into a downstream consumer.
    try std.testing.expect(std.mem.indexOf(u8, payload, "\\u0000") != null);
    try std.testing.expect(std.mem.indexOf(u8, payload, "\\u0007") != null);
    try std.testing.expect(std.mem.indexOf(u8, payload, "\\u0008") != null);
    try std.testing.expect(std.mem.indexOf(u8, payload, "\\u000b") != null);
    try std.testing.expect(std.mem.indexOf(u8, payload, "\\u000c") != null);
    try std.testing.expect(std.mem.indexOf(u8, payload, "\\u000f") != null);
    try std.testing.expect(std.mem.indexOf(u8, payload, "\\u007f") != null);
    try std.testing.expect(std.mem.indexOf(u8, payload, "\\n") != null);
    // And no bare control byte leaked through.
    for (payload) |c| {
        if (c < 0x20 or c == 0x7f) {
            std.debug.print("raw control byte 0x{x} leaked in payload: {s}\n", .{ c, payload });
            try std.testing.expect(false);
        }
    }
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
