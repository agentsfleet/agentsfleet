//! GitHub pull request operations via the REST API.
//!
//! Exports:
//!   - `HttpResponseParts`        — header/body split of a raw HTTP response.
//!   - `splitHttpResponse`        — split a raw curl `-i` response.
//!   - `parseHttpStatus`          — extract the last HTTP status code from headers.
//!   - `parseGitHubOwnerRepo`     — extract `owner/repo` from a GitHub URL.
//!   - `createPullRequest`        — open a new pull request; returns caller-owned URL.
//!   - `findOpenPullRequestByHead`— look up an existing open PR; returns caller-owned URL or null.
//!
//! Ownership:
//!   - `createPullRequest` returns a caller-owned `[]u8`; caller must free.
//!   - `findOpenPullRequestByHead` returns a caller-owned `?[]u8`; caller must free when non-null.
//!   - `error_detail_out` (when non-null) is set to a caller-owned `[]u8`; caller must free.
//!   - `parseGitHubOwnerRepo` returns a caller-owned `[]u8`; caller must free.

const std = @import("std");
const GitError = @import("errors.zig").GitError;
const validate = @import("validate.zig");
const command = @import("command.zig");
const log = std.log.scoped(.git);

pub const HttpResponseParts = struct {
    headers: []const u8,
    body: []const u8,
};

pub fn splitHttpResponse(response: []const u8) HttpResponseParts {
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

pub fn parseHttpStatus(headers: []const u8) ?u16 {
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

/// Parse `owner/repo` from a GitHub HTTPS or SSH URL.
/// Returns a caller-owned `[]u8`.
pub fn parseGitHubOwnerRepo(alloc: std.mem.Allocator, repo_url: []const u8) ![]const u8 {
    const github_prefix_https = "https://github.com/";
    const github_prefix_ssh = "git@github.com:";

    var rest: []const u8 = undefined;
    if (std.mem.startsWith(u8, repo_url, github_prefix_https)) {
        rest = repo_url[github_prefix_https.len..];
    } else if (std.mem.startsWith(u8, repo_url, github_prefix_ssh)) {
        rest = repo_url[github_prefix_ssh.len..];
    } else {
        return GitError.InvalidGitHubUrl;
    }

    // Strip .git suffix if present
    const owner_repo = if (std.mem.endsWith(u8, rest, ".git"))
        rest[0 .. rest.len - 4]
    else
        rest;

    const slash = std.mem.indexOfScalar(u8, owner_repo, '/') orelse return GitError.InvalidGitHubUrl;
    if (std.mem.lastIndexOfScalar(u8, owner_repo, '/') != slash) return GitError.InvalidGitHubUrl;

    const owner = owner_repo[0..slash];
    const repo = owner_repo[slash + 1 ..];
    if (!validate.isSafeIdentifierSegment(owner) or !validate.isSafeIdentifierSegment(repo)) return GitError.InvalidGitHubUrl;

    return alloc.dupe(u8, owner_repo);
}

/// Create a GitHub pull request via REST API.
/// Returns the PR URL (caller owns).
pub fn createPullRequest(
    alloc: std.mem.Allocator,
    repo_url: []const u8,
    branch: []const u8,
    base_branch: []const u8,
    title: []const u8,
    body: []const u8,
    github_token: []const u8,
    error_detail_out: ?*?[]u8,
) ![]const u8 {
    if (error_detail_out) |out| out.* = null;
    if (!validate.isSafeGitRef(branch) or !validate.isSafeGitRef(base_branch)) return GitError.InvalidGitRef;

    const owner_repo = try parseGitHubOwnerRepo(alloc, repo_url);
    defer alloc.free(owner_repo);

    const api_url = try std.fmt.allocPrint(
        alloc,
        "https://api.github.com/repos/{s}/pulls",
        .{owner_repo},
    );
    defer alloc.free(api_url);

    const payload = try std.json.Stringify.valueAlloc(alloc, .{
        .title = title,
        .head = branch,
        .base = base_branch,
        .body = body,
    }, .{});
    defer alloc.free(payload);

    const auth_header = try std.fmt.allocPrint(alloc, "Authorization: Bearer {s}", .{github_token});
    defer alloc.free(auth_header);

    const result = command.run(alloc, &.{
        "curl",                                "-sS",
        "--connect-timeout",                   "10",
        "--max-time",                          "30",
        "-i",                                  "-X",
        "POST",                                "-H",
        "Content-Type: application/json",      "-H",
        "Accept: application/vnd.github+json", "-H",
        auth_header,                           "-d",
        payload,                               api_url,
    }, null, 30_000) catch return GitError.PrFailed;
    defer alloc.free(result);

    const parsed_response = splitHttpResponse(result);
    const status = parseHttpStatus(parsed_response.headers) orelse {
        log.err("git.pr_status_parse_fail body={s}", .{parsed_response.body});
        return GitError.PrFailed;
    };

    if (status >= 400) {
        if (error_detail_out) |out| {
            out.* = alloc.dupe(u8, result) catch null;
        }
        log.err("git.pr_creation_fail status={d} body={s}", .{ status, parsed_response.body });
        return switch (status) {
            429 => GitError.PrRateLimited,
            401, 403 => GitError.PrAuthFailed,
            400, 422 => GitError.PrInvalidRequest,
            500...599 => GitError.PrServerError,
            else => GitError.PrFailed,
        };
    }

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, parsed_response.body, .{});
    defer parsed.deinit();

    if (parsed.value.object.get("html_url")) |url_val| {
        return alloc.dupe(u8, url_val.string);
    }

    log.err("git.pr_missing_url response={s}", .{result});
    return GitError.PrFailed;
}

/// Find an already-open pull request URL for owner:branch.
/// Returns null when no PR exists.  Caller owns returned slice.
pub fn findOpenPullRequestByHead(
    alloc: std.mem.Allocator,
    repo_url: []const u8,
    branch: []const u8,
    github_token: []const u8,
    error_detail_out: ?*?[]u8,
) !?[]const u8 {
    if (error_detail_out) |out| out.* = null;
    if (!validate.isSafeGitRef(branch)) return GitError.InvalidGitRef;

    const owner_repo = try parseGitHubOwnerRepo(alloc, repo_url);
    defer alloc.free(owner_repo);

    const slash = std.mem.indexOfScalar(u8, owner_repo, '/') orelse return GitError.InvalidGitHubUrl;
    const owner = owner_repo[0..slash];

    const head_selector = try std.fmt.allocPrint(alloc, "head={s}:{s}", .{ owner, branch });
    defer alloc.free(head_selector);

    const api_url = try std.fmt.allocPrint(
        alloc,
        "https://api.github.com/repos/{s}/pulls",
        .{owner_repo},
    );
    defer alloc.free(api_url);

    const auth_header = try std.fmt.allocPrint(alloc, "Authorization: Bearer {s}", .{github_token});
    defer alloc.free(auth_header);

    const result = command.run(alloc, &.{
        "curl",              "-sS",
        "--connect-timeout", "10",
        "--max-time",        "30",
        "-i",                "-G",
        "-H",                "Accept: application/vnd.github+json",
        "-H",                auth_header,
        "--data-urlencode",  "state=open",
        "--data-urlencode",  head_selector,
        api_url,
    }, null, 30_000) catch return GitError.PrFailed;
    defer alloc.free(result);

    const parsed_response = splitHttpResponse(result);
    const status = parseHttpStatus(parsed_response.headers) orelse return GitError.PrFailed;
    if (status >= 400) {
        if (error_detail_out) |out| out.* = alloc.dupe(u8, result) catch null;
        return switch (status) {
            429 => GitError.PrRateLimited,
            401, 403 => GitError.PrAuthFailed,
            400, 404, 422 => GitError.PrInvalidRequest,
            500...599 => GitError.PrServerError,
            else => GitError.PrFailed,
        };
    }

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, parsed_response.body, .{});
    defer parsed.deinit();
    if (parsed.value != .array or parsed.value.array.items.len == 0) return null;

    const first = parsed.value.array.items[0];
    if (first != .object) return null;
    const html_url = first.object.get("html_url") orelse return null;
    if (html_url != .string) return null;
    const url_copy = try alloc.dupe(u8, html_url.string);
    return url_copy;
}

// ---------------------------------------------------------------------------
// Tests (moved from ops.zig)
// ---------------------------------------------------------------------------

test "parseHttpStatus handles standard status lines" {
    const headers =
        "HTTP/1.1 100 Continue\r\n\r\n" ++
        "HTTP/2 429\r\nRetry-After: 7\r\n";
    try std.testing.expectEqual(@as(?u16, 429), parseHttpStatus(headers));
}

test "splitHttpResponse separates headers and body" {
    const raw = "HTTP/2 201\r\nContent-Type: application/json\r\n\r\n{\"html_url\":\"https://x\"}";
    const parts = splitHttpResponse(raw);
    try std.testing.expect(std.mem.containsAtLeast(u8, parts.headers, 1, "HTTP/2 201"));
    try std.testing.expect(std.mem.eql(u8, parts.body, "{\"html_url\":\"https://x\"}"));
}

test "parseGitHubOwnerRepo rejects malformed owner/repo path" {
    try std.testing.expectError(
        GitError.InvalidGitHubUrl,
        parseGitHubOwnerRepo(std.testing.allocator, "https://github.com/owner/repo/extra.git"),
    );
    try std.testing.expectError(
        GitError.InvalidGitHubUrl,
        parseGitHubOwnerRepo(std.testing.allocator, "https://github.com/owner/../repo.git"),
    );
}

test "integration: findOpenPullRequestByHead rejects invalid branch ref" {
    try std.testing.expectError(
        GitError.InvalidGitRef,
        findOpenPullRequestByHead(std.testing.allocator, "https://github.com/org/repo.git", "../bad", "token", null),
    );
}

// New T3 tests per spec §4.0
test "createPullRequest with invalid base_branch returns InvalidGitRef" {
    try std.testing.expectError(
        GitError.InvalidGitRef,
        createPullRequest(
            std.testing.allocator,
            "https://github.com/org/repo.git",
            "feature/valid-branch",
            "../bad-base",
            "title",
            "body",
            "token",
            null,
        ),
    );
}

test "createPullRequest with invalid branch returns InvalidGitRef" {
    try std.testing.expectError(
        GitError.InvalidGitRef,
        createPullRequest(
            std.testing.allocator,
            "https://github.com/org/repo.git",
            "../bad-branch",
            "main",
            "title",
            "body",
            "token",
            null,
        ),
    );
}
