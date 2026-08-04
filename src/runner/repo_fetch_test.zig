//! Unit tests for the repository-fetch refusal surface (`repo_fetch.zig`).
//!
//! Everything here runs with no filesystem, no subprocess, and no network —
//! which is the point. Dimension 4.6a asks that an out-of-binding fetch be
//! "refused before any network call"; a pure decision function makes that a
//! property of the code rather than something a test has to observe.

const std = @import("std");
const repo_fetch = @import("repo_fetch.zig");
const execution_policy = @import("contract").execution_policy;

const Refusal = repo_fetch.Refusal;

/// A real-looking SHA-1 object id and its SHA-256 sibling.
const SHA1 = "9a078a7b5c1d4e2f60718293a4b5c6d7e8f90123";
const SHA256 = "9a078a7b5c1d4e2f60718293a4b5c6d7e8f901239a078a7b5c1d4e2f60718293";

fn bindingOf(repos: []const []const u8, access: execution_policy.RepositoryAccess) execution_policy.RepositoryBinding {
    return .{ .repositories = repos, .access = access };
}

fn expectRefused(expected: Refusal, verdict: repo_fetch.Verdict) !void {
    switch (verdict) {
        .refused => |actual| try std.testing.expectEqual(expected, actual),
        .approved => |a| {
            std.debug.print("expected refusal .{s}, but {s} was approved\n", .{ @tagName(expected), a.repository });
            return error.TestUnexpectedResult;
        },
    }
}

fn expectApproved(verdict: repo_fetch.Verdict) !repo_fetch.Approved {
    switch (verdict) {
        .approved => |a| return a,
        .refused => |r| {
            std.debug.print("expected approval, refused: {s}\n", .{r.reason()});
            return error.TestUnexpectedResult;
        },
    }
}

test "test_fetch_is_on_demand_and_binding_scoped" {
    // Dimension 4.6a. The repairer is bound to one repository; a well-formed ask
    // for any other is refused with no network call, because the decision is a
    // pure function of the binding and the ask.
    const repos = [_][]const u8{"acme/payments"};
    const binding = bindingOf(&repos, .write);

    const ok = try expectApproved(repo_fetch.decide(binding, .{ .repository = "acme/payments", .commit = SHA1 }));
    try std.testing.expectEqualStrings("acme/payments", ok.repository);
    try std.testing.expectEqualStrings(SHA1, ok.commit);
    try std.testing.expectEqual(execution_policy.RepositoryAccess.write, ok.access);

    // A different repository under the SAME owner — the case the vendor ring
    // cannot catch, because the mint scopes by bare name.
    try expectRefused(.outside_binding, repo_fetch.decide(binding, .{ .repository = "acme/ledger", .commit = SHA1 }));
    // A same-named repository under a DIFFERENT owner. This is the review's
    // finding made concrete: the minted token would reach `acme/payments`
    // regardless of the owner asked for, so only the full-name check refuses it.
    try expectRefused(.outside_binding, repo_fetch.decide(binding, .{ .repository = "otherorg/payments", .commit = SHA1 }));
}

test "a lease carrying no binding fetches nothing" {
    // Fail closed, and for the same reason the mint fails closed: a fleet that
    // declared no repositories has no token either, so there is nothing to
    // authenticate the fetch with even if it were allowed.
    try expectRefused(.no_binding, repo_fetch.decide(null, .{ .repository = "acme/payments", .commit = SHA1 }));
}

test "the binding's spelling wins over the model's" {
    // An owner and a repository name are case-insensitively unique at the
    // vendor, so these name ONE repository and refusing the ask would be wrong.
    // What is carried forward is the operator's declared spelling, not the
    // model's — that is what reaches the remote URL, the workspace path, and the
    // log, so none of them can be steered by how the ask was capitalized.
    const repos = [_][]const u8{"Acme/Payments"};
    const ok = try expectApproved(repo_fetch.decide(
        bindingOf(&repos, .read),
        .{ .repository = "acme/payments", .commit = SHA1 },
    ));
    try std.testing.expectEqualStrings("Acme/Payments", ok.repository);
    try std.testing.expectEqual(execution_policy.RepositoryAccess.read, ok.access);
}

test "a malformed repository is refused before the binding is even consulted" {
    // Every one of these is refused whether or not it "matches" — the shape check
    // runs first, so a binding can never be tricked into admitting a name that
    // would then be interpolated into a URL or a path.
    const repos = [_][]const u8{ "acme/payments", "../../etc/passwd" };
    const binding = bindingOf(&repos, .write);
    const malformed = [_][]const u8{
        "payments", // bare name, no owner
        "acme/", // empty repository segment
        "/payments", // empty owner segment
        "acme/pay/ments", // a path, not a repository
        "../../etc/passwd", // traversal — refused even though the binding names it
        "acme/..", // traversal in the repository segment
        "acme/.hidden", // leading dot would hide the fetch target
        "https://github.com/acme/payments", // a URL, not a name
        "acme/pay ments", // whitespace
        "acme/pay\nments", // control byte
        "acme/pay;rm -rf /", // shell metacharacters
        "acme/pay$(whoami)", // command substitution
        "", // empty
    };
    for (malformed) |name| {
        try expectRefused(.malformed_repository, repo_fetch.decide(binding, .{ .repository = name, .commit = SHA1 }));
    }

    // Length ceilings, both segments and the pair.
    const long_segment = "a" ** 101;
    try expectRefused(.malformed_repository, repo_fetch.decide(binding, .{ .repository = "acme/" ++ long_segment, .commit = SHA1 }));
    try expectRefused(.malformed_repository, repo_fetch.decide(binding, .{ .repository = long_segment ++ "/payments", .commit = SHA1 }));
}

test "only a full lowercase object id names the commit to revert" {
    const repos = [_][]const u8{"acme/payments"};
    const binding = bindingOf(&repos, .write);

    // SHA-256 repositories are accepted too.
    _ = try expectApproved(repo_fetch.decide(binding, .{ .repository = "acme/payments", .commit = SHA256 }));

    const malformed = [_][]const u8{
        "9a078a7", // abbreviated: ambiguous by construction
        "main", // a branch names different bytes over time
        "v1.2.3", // so does a tag
        "HEAD", // and so does a symbolic ref
        "9A078A7B5C1D4E2F60718293A4B5C6D7E8F90123", // uppercase is a second spelling
        "9a078a7b5c1d4e2f60718293a4b5c6d7e8f9012", // 39 characters
        "9a078a7b5c1d4e2f60718293a4b5c6d7e8f901234", // 41 characters
        "9a078a7b5c1d4e2f60718293a4b5c6d7e8f9012z", // non-hexadecimal
        "",
    };
    for (malformed) |commit| {
        try expectRefused(.malformed_commit, repo_fetch.decide(binding, .{ .repository = "acme/payments", .commit = commit }));
    }
}

test "a target head is optional, and validated when given" {
    const repos = [_][]const u8{"acme/payments"};
    const binding = bindingOf(&repos, .write);

    // Absent head: the fetch will use the remote's default.
    const defaulted = try expectApproved(repo_fetch.decide(binding, .{ .repository = "acme/payments", .commit = SHA1 }));
    try std.testing.expectEqualStrings("", defaulted.head);

    const named = try expectApproved(repo_fetch.decide(binding, .{ .repository = "acme/payments", .commit = SHA1, .head = "release-2.0" }));
    try std.testing.expectEqualStrings("release-2.0", named.head);

    const malformed = [_][]const u8{
        "--upload-pack=sh", // must not be mistakable for an option
        "-rf",
        "feat/..", // traversal
        "../main",
        "feat/new-thing", // a separator would let a branch be spelled as a path
        "main branch", // whitespace
        "main\n", // control byte
        ".hidden",
        "a" ** 256, // over the ceiling
    };
    for (malformed) |head| {
        try expectRefused(.malformed_head, repo_fetch.decide(binding, .{ .repository = "acme/payments", .commit = SHA1, .head = head }));
    }
}

test "every refusal carries a distinct, readable reason" {
    // The child reformulates against this string and an operator reads it in the
    // activity stream, so an empty or duplicated reason is a real defect.
    const all = [_]Refusal{ .no_binding, .outside_binding, .malformed_repository, .malformed_commit, .malformed_head };
    for (all, 0..) |a, i| {
        try std.testing.expect(a.reason().len > 0);
        for (all[i + 1 ..]) |b| {
            try std.testing.expect(!std.mem.eql(u8, a.reason(), b.reason()));
        }
    }
}
