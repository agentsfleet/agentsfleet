//! GitHub App authentication helpers.
//! Generates RS256 JWTs and exchanges installation tokens.

const std = @import("std");
const log = std.log.scoped(.github_auth);

const GitHubAuthError = error{
    MissingConfig,
    OpenSslFailed,
    CurlFailed,
    InvalidResponse,
    CommandTimedOut,
    RateLimited,
    AuthFailed,
    InvalidRequest,
    ServerError,
    InvalidInstallationId,
};

const TokenCache = struct {
    alloc: std.mem.Allocator,
    app_id: []const u8,
    private_key_pem: []const u8,
    mutex: std.Thread.Mutex = .{},
    cached_token: ?[]u8 = null,
    refresh_deadline_ms: i64 = 0,

    pub fn init(alloc: std.mem.Allocator, app_id: []const u8, private_key_pem: []const u8) TokenCache {
        return .{
            .alloc = alloc,
            .app_id = app_id,
            .private_key_pem = private_key_pem,
        };
    }

    pub fn deinit(self: *TokenCache) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        if (self.cached_token) |token| self.alloc.free(token);
        self.cached_token = null;
        self.refresh_deadline_ms = 0;
    }

    fn getInstallationToken(
        self: *TokenCache,
        alloc: std.mem.Allocator,
        installation_id: []const u8,
    ) ![]u8 {
        return self.getInstallationTokenWithDetail(alloc, installation_id, null);
    }

    fn getInstallationTokenWithDetail(
        self: *TokenCache,
        alloc: std.mem.Allocator,
        installation_id: []const u8,
        error_detail_out: ?*?[]u8,
    ) ![]u8 {
        if (error_detail_out) |out| out.* = null;
        if (self.app_id.len == 0 or self.private_key_pem.len == 0 or installation_id.len == 0) {
            return GitHubAuthError.MissingConfig;
        }
        if (!isSafeInstallationId(installation_id)) return GitHubAuthError.InvalidInstallationId;

        self.mutex.lock();
        defer self.mutex.unlock();

        const now_ms = std.time.milliTimestamp();
        if (self.cached_token) |token| {
            if (now_ms < self.refresh_deadline_ms) {
                log.debug("token cache hit installation_id={s}", .{installation_id});
                return alloc.dupe(u8, token);
            }
            log.debug("token cache expired installation_id={s}", .{installation_id});
        }

        const jwt = try buildAppJwt(alloc, self.app_id, self.private_key_pem);
        defer alloc.free(jwt);

        const fresh_token = try exchangeInstallationToken(alloc, installation_id, jwt, error_detail_out);
        errdefer alloc.free(fresh_token);

        if (self.cached_token) |token| self.alloc.free(token);
        self.cached_token = try self.alloc.dupe(u8, fresh_token);

        // GitHub installation tokens expire in ~60 minutes; refresh 5 minutes early.
        self.refresh_deadline_ms = now_ms + (55 * std.time.ms_per_min);

        log.info("installation token refreshed installation_id={s}", .{installation_id});
        return fresh_token;
    }
};

fn encodeBase64Url(alloc: std.mem.Allocator, bytes: []const u8) ![]u8 {
    const len = std.base64.url_safe_no_pad.Encoder.calcSize(bytes.len);
    const out = try alloc.alloc(u8, len);
    _ = std.base64.url_safe_no_pad.Encoder.encode(out, bytes);
    return out;
}

fn normalizedPrivateKeyPem(alloc: std.mem.Allocator, pem: []const u8) ![]u8 {
    if (std.mem.indexOf(u8, pem, "\\n") == null) {
        return alloc.dupe(u8, pem);
    }

    var out: std.ArrayList(u8) = .{};
    errdefer out.deinit(alloc);

    var i: usize = 0;
    while (i < pem.len) : (i += 1) {
        if (pem[i] == '\\' and i + 1 < pem.len and pem[i + 1] == 'n') {
            try out.append(alloc, '\n');
            i += 1;
            continue;
        }
        try out.append(alloc, pem[i]);
    }

    return out.toOwnedSlice(alloc);
}

fn isSafeInstallationId(installation_id: []const u8) bool {
    if (installation_id.len == 0 or installation_id.len > 20) return false;
    for (installation_id) |c| {
        if (!std.ascii.isDigit(c)) return false;
    }
    return true;
}

fn randomSuffix() u64 {
    var b: [8]u8 = undefined;
    std.crypto.random.bytes(&b);
    return std.mem.readInt(u64, &b, .little);
}

fn runWithInput(
    alloc: std.mem.Allocator,
    argv: []const []const u8,
    stdin_data: ?[]const u8,
    timeout_ms: u64,
) ![]u8 {
    _ = timeout_ms;
    var child = std.process.Child.init(argv, alloc);
    var env = try sanitizedChildEnv(alloc);
    defer env.deinit();
    child.env_map = &env;
    child.stdin_behavior = if (stdin_data != null) .Pipe else .Ignore;
    child.stdout_behavior = .Pipe;
    child.stderr_behavior = .Pipe;

    try child.spawn();

    if (stdin_data) |in| {
        if (child.stdin) |*stdin_pipe| {
            try stdin_pipe.writeAll(in);
            stdin_pipe.close();
            child.stdin = null;
        }
    }

    const term = try child.wait();
    const stdout = if (child.stdout) |*s|
        try s.readToEndAlloc(alloc, 1024 * 1024)
    else
        try alloc.dupe(u8, "");
    const stderr = if (child.stderr) |*s|
        try s.readToEndAlloc(alloc, 1024 * 1024)
    else
        try alloc.dupe(u8, "");
    defer alloc.free(stderr);

    switch (term) {
        .Exited => |code| if (code != 0) {
            log.err("command failed code={d} argv[0]={s} stderr={s}", .{ code, argv[0], stderr });
            alloc.free(stdout);
            if (std.mem.eql(u8, argv[0], "curl")) return GitHubAuthError.CurlFailed;
            return GitHubAuthError.OpenSslFailed;
        },
        else => {
            alloc.free(stdout);
            if (std.mem.eql(u8, argv[0], "curl")) return GitHubAuthError.CurlFailed;
            return GitHubAuthError.OpenSslFailed;
        },
    }

    return stdout;
}

fn sanitizedChildEnv(alloc: std.mem.Allocator) !std.process.EnvMap {
    var env = std.process.EnvMap.init(alloc);
    errdefer env.deinit();

    try copyEnvIfPresent(alloc, &env, "PATH");
    try copyEnvIfPresent(alloc, &env, "HOME");
    try copyEnvIfPresent(alloc, &env, "TMPDIR");
    try copyEnvIfPresent(alloc, &env, "SSL_CERT_FILE");
    try copyEnvIfPresent(alloc, &env, "SSL_CERT_DIR");
    return env;
}

fn copyEnvIfPresent(alloc: std.mem.Allocator, env: *std.process.EnvMap, key: []const u8) !void {
    const value = std.process.getEnvVarOwned(alloc, key) catch return;
    defer alloc.free(value);
    try env.put(key, value);
}

fn signRs256(
    alloc: std.mem.Allocator,
    signing_input: []const u8,
    private_key_pem: []const u8,
) ![]u8 {
    const normalized = try normalizedPrivateKeyPem(alloc, private_key_pem);
    defer alloc.free(normalized);

    const key_path = try std.fmt.allocPrint(
        alloc,
        "/tmp/usezombie-gh-app-key-{x}.pem",
        .{randomSuffix()},
    );
    defer alloc.free(key_path);
    defer std.fs.deleteFileAbsolute(key_path) catch {};

    {
        const f = try std.fs.createFileAbsolute(key_path, .{ .mode = 0o600 });
        defer f.close();
        try f.writeAll(normalized);
    }

    return runWithInput(
        alloc,
        &.{ "openssl", "dgst", "-sha256", "-sign", key_path },
        signing_input,
        10_000,
    );
}

fn buildAppJwt(alloc: std.mem.Allocator, app_id: []const u8, private_key_pem: []const u8) ![]u8 {
    log.debug("building app jwt app_id={s}", .{app_id});
    const now = std.time.timestamp();

    const header_json = "{\"alg\":\"RS256\",\"typ\":\"JWT\"}";
    const payload_json = try std.fmt.allocPrint(
        alloc,
        "{{\"iss\":\"{s}\",\"iat\":{d},\"exp\":{d}}}",
        .{ app_id, now - 30, now + 540 },
    );
    defer alloc.free(payload_json);

    const header_b64 = try encodeBase64Url(alloc, header_json);
    defer alloc.free(header_b64);
    const payload_b64 = try encodeBase64Url(alloc, payload_json);
    defer alloc.free(payload_b64);

    const signing_input = try std.fmt.allocPrint(alloc, "{s}.{s}", .{ header_b64, payload_b64 });
    defer alloc.free(signing_input);

    const signature = try signRs256(alloc, signing_input, private_key_pem);
    defer alloc.free(signature);

    const sig_b64 = try encodeBase64Url(alloc, signature);
    defer alloc.free(sig_b64);

    return std.fmt.allocPrint(alloc, "{s}.{s}", .{ signing_input, sig_b64 });
}

fn exchangeInstallationToken(
    alloc: std.mem.Allocator,
    installation_id: []const u8,
    app_jwt: []const u8,
    error_detail_out: ?*?[]u8,
) ![]u8 {
    if (error_detail_out) |out| out.* = null;

    const url = try std.fmt.allocPrint(
        alloc,
        "https://api.github.com/app/installations/{s}/access_tokens",
        .{installation_id},
    );
    defer alloc.free(url);

    const auth = try std.fmt.allocPrint(alloc, "Authorization: Bearer {s}", .{app_jwt});
    defer alloc.free(auth);

    const response = try runWithInput(alloc, &.{
        "curl",                                "-sS",
        "--connect-timeout",                   "10",
        "--max-time",                          "30",
        "-i",                                  "-X",
        "POST",                                "-H",
        "Accept: application/vnd.github+json", "-H",
        auth,                                  url,
    }, null, 30_000);
    defer alloc.free(response);

    const parts = splitHttpResponse(response);
    const status = parseHttpStatus(parts.headers) orelse {
        log.err("invalid github token response status headers={s}", .{parts.headers});
        return GitHubAuthError.InvalidResponse;
    };

    if (status >= 400) {
        if (error_detail_out) |out| out.* = alloc.dupe(u8, response) catch null;
        log.err("installation token request failed status={d} body={s}", .{ status, parts.body });
        return classifyHttpStatus(status);
    }

    const parsed = std.json.parseFromSlice(std.json.Value, alloc, parts.body, .{}) catch {
        log.err("invalid github token response: {s}", .{parts.body});
        return GitHubAuthError.InvalidResponse;
    };
    defer parsed.deinit();

    const token_val = parsed.value.object.get("token") orelse {
        log.err("token field missing in response: {s}", .{parts.body});
        return GitHubAuthError.InvalidResponse;
    };

    return alloc.dupe(u8, token_val.string);
}

fn classifyHttpStatus(status: u16) GitHubAuthError {
    return switch (status) {
        429 => GitHubAuthError.RateLimited,
        401, 403 => GitHubAuthError.AuthFailed,
        400, 404, 422 => GitHubAuthError.InvalidRequest,
        500...599 => GitHubAuthError.ServerError,
        else => GitHubAuthError.CurlFailed,
    };
}

const HttpResponseParts = struct {
    headers: []const u8,
    body: []const u8,
};

fn splitHttpResponse(response: []const u8) HttpResponseParts {
    if (std.mem.lastIndexOf(u8, response, "\r\n\r\n")) |sep| {
        return .{
            .headers = response[0..sep],
            .body = response[sep + 4 ..],
        };
    }
    if (std.mem.lastIndexOf(u8, response, "\n\n")) |sep| {
        return .{
            .headers = response[0..sep],
            .body = response[sep + 2 ..],
        };
    }
    return .{
        .headers = "",
        .body = response,
    };
}

fn parseHttpStatus(headers: []const u8) ?u16 {
    const idx = std.mem.lastIndexOf(u8, headers, "HTTP/") orelse return null;
    const line = headers[idx..];
    const line_end = std.mem.indexOfScalar(u8, line, '\n') orelse line.len;
    const status_line = std.mem.trim(u8, line[0..line_end], " \r\n\t");

    const first_space = std.mem.indexOfScalar(u8, status_line, ' ') orelse return null;
    var rest = status_line[first_space + 1 ..];
    rest = std.mem.trimLeft(u8, rest, " ");
    if (rest.len < 3) return null;

    return std.fmt.parseInt(u16, rest[0..3], 10) catch null;
}

test "parseHttpStatus handles latest status line" {
    const headers =
        "HTTP/1.1 100 Continue\r\n\r\n" ++
        "HTTP/2 429\r\nRetry-After: 7\r\n";
    try std.testing.expectEqual(@as(?u16, 429), parseHttpStatus(headers));
}

test "splitHttpResponse separates headers and body" {
    const raw = "HTTP/2 201\r\nContent-Type: application/json\r\n\r\n{\"token\":\"abc\"}";
    const parts = splitHttpResponse(raw);
    try std.testing.expect(std.mem.containsAtLeast(u8, parts.headers, 1, "HTTP/2 201"));
    try std.testing.expect(std.mem.eql(u8, parts.body, "{\"token\":\"abc\"}"));
}

test "classifyHttpStatus maps retry and auth errors" {
    try std.testing.expectEqual(GitHubAuthError.RateLimited, classifyHttpStatus(429));
    try std.testing.expectEqual(GitHubAuthError.AuthFailed, classifyHttpStatus(401));
    try std.testing.expectEqual(GitHubAuthError.InvalidRequest, classifyHttpStatus(422));
    try std.testing.expectEqual(GitHubAuthError.ServerError, classifyHttpStatus(503));
}

test "integration: sanitizedChildEnv does not leak auth env vars" {
    var env = try sanitizedChildEnv(std.testing.allocator);
    defer env.deinit();

    try std.testing.expect(env.get("GITHUB_TOKEN") == null);
    try std.testing.expect(env.get("GIT_ASKPASS") == null);
    try std.testing.expect(env.get("ENCRYPTION_MASTER_KEY") == null);
}

test "isSafeInstallationId accepts digits only" {
    try std.testing.expect(isSafeInstallationId("12345"));
    try std.testing.expect(!isSafeInstallationId(""));
    try std.testing.expect(!isSafeInstallationId("abc"));
    try std.testing.expect(!isSafeInstallationId("123-45"));
}

test "integration: getInstallationToken rejects invalid installation id" {
    var cache = TokenCache.init(std.testing.allocator, "42", "pem");
    defer cache.deinit();
    try std.testing.expectError(
        GitHubAuthError.InvalidInstallationId,
        cache.getInstallationToken(std.testing.allocator, "../123"),
    );
}
