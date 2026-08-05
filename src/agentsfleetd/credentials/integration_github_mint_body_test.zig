//! The installation-token request BODY — repository and permission scoping.
//!
//! Split from `integration_github.zig` for the file-length budget (RULE FLL), and
//! deliberately driven through the public `mint` rather than the private builder:
//! what protects a repository is the bytes that reach GitHub, so the assertions
//! read `FakeGitHub.body` — the captured outbound payload — not an intermediate.
//!
//! Why this matters: the prior mint posted an EMPTY body, which asks GitHub for
//! the App installation's full permission set across every repository it covers,
//! valid for an hour. Every test here pins one half of the narrowing — the
//! repositories named, and how far the permissions reach.

const std = @import("std");
const integration = @import("integration.zig");
const testing = @import("testing.zig");
const github = @import("integration_github.zig");

const Retry = integration.Retry;
const HANDLE_GH = "{\"integration\":\"github\",\"installation_id\":\"42\"}";
const TEST_NOW_MS: i64 = 1_700_000_000_000;

const REPOS_ONE = [_][]const u8{"acme/widgets"};
const REPOS_TWO = [_][]const u8{ "acme/widgets", "acme/gadgets" };

fn bindingOf(repos: []const []const u8, access: integration.RepositoryAccess) integration.RepositoryBinding {
    return .{ .repositories = repos, .access = access };
}

test "test_mint_body_is_repository_and_access_scoped" {
    // Dimension 2.3, write half — a write binding pins the repository and grants
    // contents + pull_requests. The read half is the sibling test below.
    const alloc = std.testing.allocator;
    var gh = testing.FakeGitHub{ .alloc = alloc, .status = 201 };
    defer gh.deinit();
    var h = try testing.parse(alloc, HANDLE_GH);
    defer h.deinit();

    const out = try github.mint(testing.githubCtxBound(alloc, h.value, &gh, TEST_NOW_MS, bindingOf(&REPOS_ONE, .write)));
    try std.testing.expect(out == .ok);
    alloc.free(out.ok.token);

    // GitHub scopes by repository NAME — the installation already fixes the
    // owner, so the qualified `acme/widgets` must have been reduced to `widgets`.
    // Asserting the slashed form is ABSENT is the load-bearing half: sending it
    // would match no repository and yield a token scoped to nothing.
    try std.testing.expect(std.mem.indexOf(u8, gh.body, "\"widgets\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, gh.body, "acme/widgets") == null);
    try std.testing.expect(std.mem.indexOf(u8, gh.body, "\"contents\":\"write\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, gh.body, "\"pull_requests\":\"write\"") != null);
}

test "mint body: a read binding grants contents:read and NO pull_requests key at all" {
    const alloc = std.testing.allocator;
    var gh = testing.FakeGitHub{ .alloc = alloc, .status = 201 };
    defer gh.deinit();
    var h = try testing.parse(alloc, HANDLE_GH);
    defer h.deinit();

    const out = try github.mint(testing.githubCtxBound(alloc, h.value, &gh, TEST_NOW_MS, bindingOf(&REPOS_ONE, .read)));
    try std.testing.expect(out == .ok);
    alloc.free(out.ok.token);

    try std.testing.expect(std.mem.indexOf(u8, gh.body, "\"contents\":\"read\"") != null);
    // Absent, not present-and-read. This is what makes the investigator unable to
    // open a Pull Request: the vendor refuses, whatever the model attempts.
    try std.testing.expect(std.mem.indexOf(u8, gh.body, "pull_requests") == null);
    try std.testing.expect(std.mem.indexOf(u8, gh.body, "write") == null);
}

test "mint body: every repository in the binding reaches the body" {
    const alloc = std.testing.allocator;
    // Two declared, so the stated reach must name both — the mint refuses a
    // token reaching fewer repositories than were asked for.
    var gh = testing.FakeGitHub{
        .alloc = alloc,
        .status = 201,
        .resp_body = "{\"token\":\"ghs_minted\",\"repositories\":" ++
            "[{\"full_name\":\"acme/widgets\"},{\"full_name\":\"acme/gadgets\"}]}",
    };
    defer gh.deinit();
    var h = try testing.parse(alloc, HANDLE_GH);
    defer h.deinit();

    const out = try github.mint(testing.githubCtxBound(alloc, h.value, &gh, TEST_NOW_MS, bindingOf(&REPOS_TWO, .write)));
    try std.testing.expect(out == .ok);
    alloc.free(out.ok.token);

    try std.testing.expect(std.mem.indexOf(u8, gh.body, "\"widgets\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, gh.body, "\"gadgets\"") != null);
}

test "test_unbound_fleet_mints_nothing" {
    // Dimension 2.4. A fleet with no repository binding mints nothing, and the
    // refusal happens before GitHub is ever called.
    const alloc = std.testing.allocator;
    var gh = testing.FakeGitHub{ .alloc = alloc, .status = 201 };
    defer gh.deinit();
    var h = try testing.parse(alloc, HANDLE_GH);
    defer h.deinit();

    const out = try github.mint(testing.githubCtxBound(alloc, h.value, &gh, TEST_NOW_MS, null));
    try std.testing.expect(out == .mint_failed);
    try std.testing.expectEqual(Retry.permanent, out.mint_failed);
    // Refused BEFORE the exchange — no request was ever made, so no token exists
    // upstream to leak or revoke. An empty captured body proves the post never ran.
    try std.testing.expectEqual(@as(usize, 0), gh.body.len);
}

test "mint body: a binding naming zero repositories is a refusal, not an all-repositories mint" {
    const alloc = std.testing.allocator;
    var gh = testing.FakeGitHub{ .alloc = alloc, .status = 201 };
    defer gh.deinit();
    var h = try testing.parse(alloc, HANDLE_GH);
    defer h.deinit();

    const empty = [_][]const u8{};
    const out = try github.mint(testing.githubCtxBound(alloc, h.value, &gh, TEST_NOW_MS, bindingOf(&empty, .write)));
    try std.testing.expect(out == .mint_failed);
    try std.testing.expectEqual(Retry.permanent, out.mint_failed);
    try std.testing.expectEqual(@as(usize, 0), gh.body.len);
}

test "mint: a token scoped to another owner's repository is refused, not returned" {
    // The owner-stripping mis-scope, driven through the public `mint` rather than
    // the pure verifier: the binding names `acme/widgets`, the wire carries the
    // bare `widgets`, and the installation account answers with ITS `megam/widgets`.
    // Before this refusal the fetch path compared `owner/repo` and the
    // `${secrets.github}` path compared nothing, so the same declaration meant
    // two different things depending on which path the model took.
    const alloc = std.testing.allocator;
    var gh = testing.FakeGitHub{
        .alloc = alloc,
        .status = 201,
        .resp_body = "{\"token\":\"ghs_minted\",\"repositories\":[{\"full_name\":\"megam/widgets\"}]}",
    };
    defer gh.deinit();
    var h = try testing.parse(alloc, HANDLE_GH);
    defer h.deinit();

    const out = try github.mint(testing.githubCtxBound(alloc, h.value, &gh, TEST_NOW_MS, bindingOf(&REPOS_ONE, .write)));
    try std.testing.expect(out == .mint_failed);
    try std.testing.expectEqual(Retry.permanent, out.mint_failed);
    // The exchange DID happen — this refusal is response-side, unlike the unbound
    // one above — but nothing usable came back out of it.
    try std.testing.expect(gh.calls > 0);
}

test "mint: a response that states no reach at all is refused" {
    // The pre-narrowing response shape. Reading a missing `repositories` array as
    // "every repository in the installation" is the exact behaviour Section 2
    // removed; it must not return by way of the parser.
    const alloc = std.testing.allocator;
    var gh = testing.FakeGitHub{
        .alloc = alloc,
        .status = 201,
        .resp_body = "{\"token\":\"ghs_minted\"}",
    };
    defer gh.deinit();
    var h = try testing.parse(alloc, HANDLE_GH);
    defer h.deinit();

    const out = try github.mint(testing.githubCtxBound(alloc, h.value, &gh, TEST_NOW_MS, bindingOf(&REPOS_ONE, .write)));
    try std.testing.expect(out == .mint_failed);
    try std.testing.expectEqual(Retry.permanent, out.mint_failed);
}

test "mint body: a repository name that is not a repository name is refused, never escaped" {
    const alloc = std.testing.allocator;
    // A quote here would break out of the JSON string if the builder escaped
    // instead of refusing. No real repository name contains one, so refusing is
    // both correct and keeps an escaping path out of the mint entirely.
    const hostile = [_][]const u8{"acme/wid\"gets"};
    var gh = testing.FakeGitHub{ .alloc = alloc, .status = 201 };
    defer gh.deinit();
    var h = try testing.parse(alloc, HANDLE_GH);
    defer h.deinit();

    const out = try github.mint(testing.githubCtxBound(alloc, h.value, &gh, TEST_NOW_MS, bindingOf(&hostile, .write)));
    try std.testing.expect(out == .mint_failed);
    try std.testing.expectEqual(Retry.permanent, out.mint_failed);
    try std.testing.expectEqual(@as(usize, 0), gh.body.len);
}
