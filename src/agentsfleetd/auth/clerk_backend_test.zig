const std = @import("std");
const cb = @import("clerk_backend.zig");

test "metadata endpoint uses PATCH (Clerk rejects POST with 405)" {
    // Regression guard: agentsfleetd silently failed every metadata writeback
    // because this constant was POST. Clerk's /v1/users/{id}/metadata
    // requires PATCH; the e2e harness surfaced the bug after seeing 405s
    // in flyctl logs.
    try std.testing.expectEqual(std.http.Method.PATCH, cb.METADATA_HTTP_METHOD);
}

test "renderMetadataPayload: both fields → compact merge body" {
    const alloc = std.testing.allocator;
    const payload = try cb.renderMetadataPayload(alloc, "0195b4ba-8d3a-7f13-8abc-aa0000000001", "fleet:admin credential:write");
    defer alloc.free(payload);
    try std.testing.expectEqualStrings(
        \\{"public_metadata":{"tenant_id":"0195b4ba-8d3a-7f13-8abc-aa0000000001","scopes":"fleet:admin credential:write"}}
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

test "renderMetadataPayload: scopes only" {
    const alloc = std.testing.allocator;
    const payload = try cb.renderMetadataPayload(alloc, null, "workspace:admin");
    defer alloc.free(payload);
    try std.testing.expectEqualStrings(
        \\{"public_metadata":{"scopes":"workspace:admin"}}
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
    // Option C: the secret is a typed argument (Context field), not an env read.
    // Passing null drives the fail-closed path deterministically — a missing key
    // must MissingSecret rather than make an unauthenticated outbound call.
    try std.testing.expectError(
        cb.PatchError.MissingSecret,
        cb.patchUserPublicMetadata(null, cb.API_BASE, std.testing.allocator, "user_test", "t_abc", "operator"),
    );
}

test "resolveApiBase: absent or blank falls to the vendor default" {
    try std.testing.expectEqualStrings(cb.API_BASE, try cb.resolveApiBase(null));
    try std.testing.expectEqualStrings(cb.API_BASE, try cb.resolveApiBase(""));
    try std.testing.expectEqualStrings(cb.API_BASE, try cb.resolveApiBase("  \t\n"));
}

test "resolveApiBase: https and loopback http pass trimmed; a cleartext remote refuses boot" {
    // A non-TLS remote base would carry the provider ADMIN secret in
    // cleartext; garbage would boot into a total auth outage. Both refuse.
    try std.testing.expectEqualStrings(
        "https://clerk.example.test/v1",
        try cb.resolveApiBase("https://clerk.example.test/v1\n"),
    );
    try std.testing.expectEqualStrings("http://127.0.0.1:8443", try cb.resolveApiBase("http://127.0.0.1:8443"));
    try std.testing.expectEqualStrings("http://localhost:9", try cb.resolveApiBase("http://localhost:9"));
    try std.testing.expectError(error.InvalidApiBase, cb.resolveApiBase("http://api.clerk.com/v1"));
    try std.testing.expectError(error.InvalidApiBase, cb.resolveApiBase("ftp://x"));
    try std.testing.expectError(error.InvalidApiBase, cb.resolveApiBase("not a url"));
}

test "resolveApiBase: loopback look-alikes cannot smuggle a remote host" {
    // Prefix-match bypasses: each of these STARTS with a loopback prefix but
    // names a remote host — accepting any of them ships the admin secret in
    // cleartext to that host.
    try std.testing.expectError(error.InvalidApiBase, cb.resolveApiBase("http://127.0.0.1.attacker.example/v1"));
    try std.testing.expectError(error.InvalidApiBase, cb.resolveApiBase("http://localhost@evil.example/v1"));
    try std.testing.expectError(error.InvalidApiBase, cb.resolveApiBase("http://localhost:9@evil.example/v1"));
    try std.testing.expectError(error.InvalidApiBase, cb.resolveApiBase("http://localhost:"));
    // And the legitimate shapes stay legitimate.
    try std.testing.expectEqualStrings("http://127.0.0.1:8443/v1", try cb.resolveApiBase("http://127.0.0.1:8443/v1"));
    try std.testing.expectEqualStrings("http://localhost/v1", try cb.resolveApiBase("http://localhost/v1"));
}
