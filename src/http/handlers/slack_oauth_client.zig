// Slack API HTTP client helpers used by the OAuth flow.
// Separated from slack_oauth.zig to keep handlers under 350 lines (RULE FLL).

const std = @import("std");

pub const SLACK_TOKEN_URL = "https://slack.com/api/oauth.v2.access";
pub const SLACK_POST_MSG_URL = "https://slack.com/api/chat.postMessage";
pub const SLACK_SCOPES = "chat:write,channels:read,channels:history,reactions:write,users:read";

pub const SLACK_CONNECTED_MSG =
    \\{"channel":"general","blocks":[{"type":"section","text":{"type":"mrkdwn","text":"*UseZombie is connected!*\nConfigure at https://app.usezombie.com to request Slack access."}}]}
;

pub const SlackTokenResponse = struct {
    access_token: []const u8,
    team_id: []const u8,
    team_name: []const u8,
    scope: []const u8, // actual granted scopes from Slack (may differ from requested)
};

/// Exchange an OAuth authorization code for a bot token via oauth.v2.access.
/// Caller owns all strings in the returned SlackTokenResponse (via alloc).
pub fn exchangeCode(alloc: std.mem.Allocator, code: []const u8, app_url: []const u8) !SlackTokenResponse {
    const client_id = std.process.getEnvVarOwned(alloc, "SLACK_CLIENT_ID") catch return error.MissingClientId;
    const client_secret = std.process.getEnvVarOwned(alloc, "SLACK_CLIENT_SECRET") catch return error.MissingClientSecret;
    const redir = try std.fmt.allocPrint(alloc, "{s}/v1/slack/callback", .{app_url});
    const redir_enc = try urlEncode(alloc, redir);
    const client_id_enc = try urlEncode(alloc, client_id);
    const client_secret_enc = try urlEncode(alloc, client_secret);
    const body = try std.fmt.allocPrint(alloc, "code={s}&client_id={s}&client_secret={s}&redirect_uri={s}", .{
        code, client_id_enc, client_secret_enc, redir_enc,
    });

    var client: std.http.Client = .{ .allocator = alloc };
    defer client.deinit();
    var resp_body: std.ArrayList(u8) = .{};
    defer resp_body.deinit(alloc);
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &resp_body);
    const result = client.fetch(.{
        .location = .{ .url = SLACK_TOKEN_URL },
        .method = .POST,
        .extra_headers = &.{.{ .name = "content-type", .value = "application/x-www-form-urlencoded" }},
        .payload = body,
        .response_writer = &aw.writer,
    }) catch return error.ExchangeHttpFailed;
    if (result.status != .ok) return error.ExchangeHttpFailed;

    const Raw = struct {
        ok: bool,
        access_token: ?[]const u8 = null,
        scope: ?[]const u8 = null,
        team: ?struct { id: []const u8, name: []const u8 } = null,
    };
    const parsed = std.json.parseFromSlice(Raw, alloc, resp_body.items, .{ .ignore_unknown_fields = true }) catch
        return error.ExchangeParseFailed;
    defer parsed.deinit();
    if (!parsed.value.ok) return error.ExchangeSlackError;
    const token = parsed.value.access_token orelse return error.ExchangeParseFailed;
    const team = parsed.value.team orelse return error.ExchangeParseFailed;
    const scope = parsed.value.scope orelse SLACK_SCOPES; // fall back to requested scopes
    return .{
        .access_token = try alloc.dupe(u8, token),
        .team_id = try alloc.dupe(u8, team.id),
        .team_name = try alloc.dupe(u8, team.name),
        .scope = try alloc.dupe(u8, scope),
    };
}

/// Post the "UseZombie is connected" confirmation message.
/// Bootstrap exception (§2.0): one direct post; all future Slack usage via M9.
pub fn postConfirmation(alloc: std.mem.Allocator, token: []const u8) !void {
    const auth = try std.fmt.allocPrint(alloc, "Bearer {s}", .{token});
    var client: std.http.Client = .{ .allocator = alloc };
    defer client.deinit();
    var resp: std.ArrayList(u8) = .{};
    defer resp.deinit(alloc);
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &resp);
    _ = client.fetch(.{
        .location = .{ .url = SLACK_POST_MSG_URL },
        .method = .POST,
        .extra_headers = &.{
            .{ .name = "authorization", .value = auth },
            .{ .name = "content-type", .value = "application/json; charset=utf-8" },
        },
        .payload = SLACK_CONNECTED_MSG,
        .response_writer = &aw.writer,
    }) catch return error.PostMessageFailed;
}

/// Percent-encode a string per RFC 3986. Caller owns the returned slice.
pub fn urlEncode(alloc: std.mem.Allocator, input: []const u8) ![]const u8 {
    var out: std.ArrayList(u8) = .{};
    defer out.deinit(alloc);
    for (input) |c| {
        switch (c) {
            'A'...'Z', 'a'...'z', '0'...'9', '-', '_', '.', '~' => try out.append(alloc, c),
            else => {
                var buf: [3]u8 = undefined;
                try out.appendSlice(alloc, try std.fmt.bufPrint(&buf, "%{X:0>2}", .{c}));
            },
        }
    }
    return out.toOwnedSlice(alloc);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test "urlEncode: safe chars pass through" {
    const alloc = std.testing.allocator;
    const out = try urlEncode(alloc, "abc-123_test.~");
    defer alloc.free(out);
    try std.testing.expectEqualStrings("abc-123_test.~", out);
}

test "urlEncode: colons and slashes encoded" {
    const alloc = std.testing.allocator;
    const out = try urlEncode(alloc, "https://x.com/path");
    defer alloc.free(out);
    try std.testing.expect(std.mem.indexOf(u8, out, "%3A") != null);
    try std.testing.expect(std.mem.indexOf(u8, out, "%2F") != null);
}
