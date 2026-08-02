//! Unit tests for apply-time bounds enforcement.
//!
//! The question under test is narrow and adversarial: given a proposal a human
//! already approved, can its diff reach a file the approval never covered? Each
//! case here is a way a diff could name a path — a plain header, a rename pair,
//! a created file, a header hidden inside hunk content — and the expected
//! answer is always the same shape: allowed, or refused with the path named.

const std = @import("std");
const bounds = @import("repair_bounds.zig");
const repair_proposal = @import("repair_proposal.zig");

const testing = std.testing;

const BASE_SHA = "0123456789abcdef0123456789abcdef01234567";

fn proposalWith(files: [][]const u8, diff: []const u8) repair_proposal.Proposal {
    return .{
        .repo = "agentsfleet/agentsfleet",
        .base_sha = BASE_SHA,
        .files = files,
        .diff = diff,
        .cause = "the checkout handler dropped its error branch",
        .evidence = &.{},
    };
}

fn expectOk(result: bounds.Result) !void {
    switch (result) {
        .ok => try testing.expect(result.refusal() == null),
        .violated => return error.TestExpectedBoundsOk,
    }
}

fn expectPathViolation(result: bounds.Result, expected_path: []const u8) !void {
    switch (result) {
        .ok => return error.TestExpectedBoundsViolation,
        .violated => |violation| switch (violation) {
            .path_outside_allowlist => |path| {
                try testing.expectEqualStrings(expected_path, path);
                try testing.expectEqual(repair_proposal.Refusal.bounds_exceeded, result.refusal().?);
            },
            else => return error.TestExpectedPathViolation,
        },
    }
}

test "test_bounds_enforced_at_apply" {
    // A diff that touches exactly what it declared.
    var allowed = [_][]const u8{"src/a.zig"};
    try expectOk(bounds.check(proposalWith(
        &allowed,
        "--- a/src/a.zig\n+++ b/src/a.zig\n@@ -1 +1 @@\n-old\n+new\n",
    )));

    // A second file the approval never covered.
    try expectPathViolation(bounds.check(proposalWith(
        &allowed,
        "--- a/src/a.zig\n+++ b/src/a.zig\n@@ -1 +1 @@\n-old\n+new\n" ++
            "--- a/src/other.zig\n+++ b/src/other.zig\n@@ -1 +1 @@\n-x\n+y\n",
    )), "src/other.zig");

    // A rename names two paths and both must be allowed — the destination is
    // where bytes actually land.
    try expectPathViolation(bounds.check(proposalWith(
        &allowed,
        "diff --git a/src/a.zig b/src/evil.zig\nsimilarity index 100%\n",
    )), "src/evil.zig");

    // Creating a file names /dev/null on the old side; only the real side has
    // to be in the allowlist.
    try expectOk(bounds.check(proposalWith(
        &allowed,
        "--- /dev/null\n+++ b/src/a.zig\n@@ -0,0 +1 @@\n+new\n",
    )));

    // Headers carrying a tab-separated timestamp still resolve to the path.
    try expectOk(bounds.check(proposalWith(
        &allowed,
        "--- a/src/a.zig\t2026-08-01 10:00:00\n+++ b/src/a.zig\t2026-08-01 10:05:00\n@@ -1 +1 @@\n-old\n+new\n",
    )));

    // Size caps refuse before any path scan.
    const huge = try testing.allocator.alloc(u8, repair_proposal.MAX_DIFF_BYTES + 1);
    defer testing.allocator.free(huge);
    @memset(huge, 'x');
    switch (bounds.check(proposalWith(&allowed, huge))) {
        .violated => |v| try testing.expect(v == .diff_too_large),
        .ok => return error.TestExpectedBoundsViolation,
    }

    var too_many: [repair_proposal.MAX_FILES + 1][]const u8 = undefined;
    for (&too_many, 0..) |*slot, i| {
        _ = i;
        slot.* = "src/a.zig";
    }
    switch (bounds.check(proposalWith(&too_many, "--- a/src/a.zig\n+++ b/src/a.zig\n"))) {
        .violated => |v| try testing.expect(v == .too_many_files),
        .ok => return error.TestExpectedBoundsViolation,
    }
}

test "test_header_hidden_in_hunk_content_is_refused" {
    // A content line that itself begins with "++ " renders as a line starting
    // with "+++ ". A strict parser reads it as hunk content; the tool applying
    // the patch may not. This check reads it as a header on purpose and
    // refuses — the conservative direction, since the other one writes a file
    // nobody approved.
    var allowed = [_][]const u8{"src/a.zig"};
    try expectPathViolation(bounds.check(proposalWith(
        &allowed,
        "--- a/src/a.zig\n+++ b/src/a.zig\n@@ -1,2 +1,2 @@\n-old\n+++ b/src/evil.zig\n",
    )), "src/evil.zig");
}
