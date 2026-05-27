//! Error classification for external calls and retry decisions.

const std = @import("std");
const types = @import("../types.zig");

pub const ErrorClass = enum {
    rate_limited,
    timeout,
    context_exhausted,
    auth,
    invalid_request,
    server_error,
    unknown,
};

const Classified = struct {
    class: ErrorClass,
    retryable: bool,
    retry_after_ms: ?u64,
    reason_code: types.ReasonCode,
};

fn findIgnoreCase(haystack: []const u8, needle: []const u8) ?usize {
    if (needle.len == 0) return 0;
    if (needle.len > haystack.len) return null;

    var i: usize = 0;
    while (i + needle.len <= haystack.len) : (i += 1) {
        if (std.ascii.eqlIgnoreCase(haystack[i .. i + needle.len], needle)) return i;
    }
    return null;
}

fn containsIgnoreCase(haystack: []const u8, needle: []const u8) bool {
    return findIgnoreCase(haystack, needle) != null;
}

fn parseRetryAfterMs(detail: []const u8) ?u64 {
    const needle = "Retry-After:";
    const idx = findIgnoreCase(detail, needle) orelse return null;

    var rest = detail[idx + needle.len ..];
    rest = std.mem.trimLeft(u8, rest, " \t");

    var end: usize = 0;
    while (end < rest.len and rest[end] >= '0' and rest[end] <= '9') : (end += 1) {}
    if (end == 0) return null;

    const seconds = std.fmt.parseInt(u64, rest[0..end], 10) catch return null;
    return seconds * std.time.ms_per_s;
}

fn fromDetail(detail: []const u8) ?Classified {
    if (containsIgnoreCase(detail, "429") or
        containsIgnoreCase(detail, "rate limit") or
        containsIgnoreCase(detail, "too many requests"))
    {
        return .{
            .class = .rate_limited,
            .retryable = true,
            .retry_after_ms = parseRetryAfterMs(detail),
            .reason_code = .RATE_LIMITED,
        };
    }

    if (containsIgnoreCase(detail, "401") or
        containsIgnoreCase(detail, "403") or
        containsIgnoreCase(detail, "unauthorized") or
        containsIgnoreCase(detail, "forbidden") or
        containsIgnoreCase(detail, "invalid token"))
    {
        return .{
            .class = .auth,
            .retryable = false,
            .retry_after_ms = null,
            .reason_code = .AUTH_FAILED,
        };
    }

    if (containsIgnoreCase(detail, "context") and containsIgnoreCase(detail, "exhaust")) {
        return .{
            .class = .context_exhausted,
            .retryable = false,
            .retry_after_ms = null,
            .reason_code = .SPEC_MISMATCH,
        };
    }

    if (containsIgnoreCase(detail, "408") or
        containsIgnoreCase(detail, "504") or
        containsIgnoreCase(detail, "timeout"))
    {
        return .{
            .class = .timeout,
            .retryable = true,
            .retry_after_ms = null,
            .reason_code = .AGENT_TIMEOUT,
        };
    }

    if (containsIgnoreCase(detail, "500") or
        containsIgnoreCase(detail, "502") or
        containsIgnoreCase(detail, "503"))
    {
        return .{
            .class = .server_error,
            .retryable = true,
            .retry_after_ms = null,
            .reason_code = .AGENT_TIMEOUT,
        };
    }

    if (containsIgnoreCase(detail, "400") or
        containsIgnoreCase(detail, "422") or
        containsIgnoreCase(detail, "bad request"))
    {
        return .{
            .class = .invalid_request,
            .retryable = false,
            .retry_after_ms = null,
            .reason_code = .AGENT_CRASH,
        };
    }

    return null;
}

pub fn classify(err: anyerror, detail: ?[]const u8) Classified {
    if (detail) |d| {
        if (fromDetail(d)) |c| return c;
    }

    const name = @errorName(err);

    if (std.mem.eql(u8, name, "CommandTimedOut") or
        std.mem.eql(u8, name, "Timeout") or
        std.mem.eql(u8, name, "TimedOut"))
    {
        return .{
            .class = .timeout,
            .retryable = true,
            .retry_after_ms = null,
            .reason_code = .AGENT_TIMEOUT,
        };
    }

    if (std.mem.eql(u8, name, "PrRateLimited")) {
        return .{
            .class = .rate_limited,
            .retryable = true,
            .retry_after_ms = if (detail) |d| parseRetryAfterMs(d) else null,
            .reason_code = .RATE_LIMITED,
        };
    }

    if (std.mem.eql(u8, name, "RateLimited")) {
        return .{
            .class = .rate_limited,
            .retryable = true,
            .retry_after_ms = if (detail) |d| parseRetryAfterMs(d) else null,
            .reason_code = .RATE_LIMITED,
        };
    }

    if (std.mem.eql(u8, name, "PrAuthFailed") or std.mem.eql(u8, name, "AuthFailed")) {
        return .{
            .class = .auth,
            .retryable = false,
            .retry_after_ms = null,
            .reason_code = .AUTH_FAILED,
        };
    }

    if (std.mem.eql(u8, name, "PrInvalidRequest") or
        std.mem.eql(u8, name, "InvalidRequest") or
        std.mem.eql(u8, name, "OpenSslFailed"))
    {
        return .{
            .class = .invalid_request,
            .retryable = false,
            .retry_after_ms = null,
            .reason_code = .AGENT_CRASH,
        };
    }

    if (std.mem.eql(u8, name, "PrServerError") or std.mem.eql(u8, name, "ServerError")) {
        return .{
            .class = .server_error,
            .retryable = true,
            .retry_after_ms = null,
            .reason_code = .AGENT_TIMEOUT,
        };
    }

    if (std.mem.eql(u8, name, "CurlFailed") or
        std.mem.eql(u8, name, "FetchFailed") or
        std.mem.eql(u8, name, "CloneFailed") or
        std.mem.eql(u8, name, "WorktreeFailed") or
        std.mem.eql(u8, name, "CommitFailed") or
        std.mem.eql(u8, name, "PushFailed") or
        std.mem.eql(u8, name, "PrFailed") or
        std.mem.eql(u8, name, "CommandFailed") or
        std.mem.eql(u8, name, "ConnectFailed"))
    {
        return .{
            .class = .server_error,
            .retryable = true,
            .retry_after_ms = null,
            .reason_code = .AGENT_TIMEOUT,
        };
    }

    if (std.mem.eql(u8, name, "MissingConfig") or
        std.mem.eql(u8, name, "MissingGitHubInstallation") or
        std.mem.eql(u8, name, "MissingMasterKey"))
    {
        return .{
            .class = .auth,
            .retryable = false,
            .retry_after_ms = null,
            .reason_code = .AUTH_FAILED,
        };
    }

    if (std.mem.eql(u8, name, "InvalidResponse")) {
        return .{
            .class = .invalid_request,
            .retryable = false,
            .retry_after_ms = null,
            .reason_code = .AGENT_CRASH,
        };
    }

    return .{
        .class = .unknown,
        .retryable = false,
        .retry_after_ms = null,
        .reason_code = .AGENT_CRASH,
    };
}

test "classify timeout errors as retryable" {
    const c = classify(error.CommandTimedOut, null);
    try std.testing.expectEqual(ErrorClass.timeout, c.class);
    try std.testing.expect(c.retryable);
}

test "classify detail-based rate limit and retry-after" {
    const c = classify(error.CurlFailed, "HTTP 429\nretry-after: 7\n");
    try std.testing.expectEqual(ErrorClass.rate_limited, c.class);
    try std.testing.expect(c.retryable);
    try std.testing.expectEqual(types.ReasonCode.RATE_LIMITED, c.reason_code);
    try std.testing.expectEqual(@as(?u64, 7_000), c.retry_after_ms);
}

test "classify detail-based auth as non-retryable" {
    const c = classify(error.CurlFailed, "HTTP 401 unauthorized");
    try std.testing.expectEqual(ErrorClass.auth, c.class);
    try std.testing.expect(!c.retryable);
    try std.testing.expectEqual(types.ReasonCode.AUTH_FAILED, c.reason_code);
}

test "classify config auth failures with auth reason code" {
    const c = classify(error.MissingConfig, null);
    try std.testing.expectEqual(ErrorClass.auth, c.class);
    try std.testing.expectEqual(types.ReasonCode.AUTH_FAILED, c.reason_code);
}

test "classify pr rate limit error with retry-after from detail" {
    const c = classify(error.PrRateLimited, "HTTP/2 429\nRetry-After: 9\n");
    try std.testing.expectEqual(ErrorClass.rate_limited, c.class);
    try std.testing.expect(c.retryable);
    try std.testing.expectEqual(@as(?u64, 9_000), c.retry_after_ms);
    try std.testing.expectEqual(types.ReasonCode.RATE_LIMITED, c.reason_code);
}

test "classify github installation token rate-limits with retry-after detail" {
    const c = classify(error.RateLimited, "HTTP/2 429\nRetry-After: 11\n");
    try std.testing.expectEqual(ErrorClass.rate_limited, c.class);
    try std.testing.expect(c.retryable);
    try std.testing.expectEqual(@as(?u64, 11_000), c.retry_after_ms);
    try std.testing.expectEqual(types.ReasonCode.RATE_LIMITED, c.reason_code);
}

test "classify git commit failures as retryable server errors" {
    const c = classify(error.CommitFailed, null);
    try std.testing.expectEqual(ErrorClass.server_error, c.class);
    try std.testing.expect(c.retryable);
    try std.testing.expectEqual(types.ReasonCode.AGENT_TIMEOUT, c.reason_code);
}
