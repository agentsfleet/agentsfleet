//! Unit tests for the repair-proposal kernel: what parses, what is refused,
//! and what the content hash promises.
//!
//! The hash tests are the load-bearing ones. Approval binds bytes, so the two
//! properties that must hold are "the same proposal always hashes the same,
//! however it was spelled" and "a changed diff never hashes the same". The
//! rest of this file is the validation boundary: everything a model can emit
//! that is not a proposal, refused by name.

const std = @import("std");
const repair_proposal = @import("repair_proposal.zig");
const ec = @import("../errors/error_registry.zig");

const testing = std.testing;

const VALID_SHA = "0123456789abcdef0123456789abcdef01234567";
const OTHER_SHA = "fedcba9876543210fedcba9876543210fedcba98";
const DEFAULT_DIFF = "--- a/src/a.zig\\n+++ b/src/a.zig\\n@@ -1 +1 @@\\n-old\\n+new\\n";
const DEFAULT_EVIDENCE = "[{\"kind\":\"esql\",\"ref\":\"FROM logs-*\",\"digest\":\"sha256:0f3a\"}]";

/// One proposal block, field by field, so a test can vary exactly one thing.
const Fields = struct {
    repo: []const u8 = "agentsfleet/agentsfleet",
    base_sha: []const u8 = VALID_SHA,
    files: []const u8 = "[\"src/a.zig\"]",
    diff: []const u8 = DEFAULT_DIFF,
    cause: []const u8 = "the checkout handler dropped its error branch",
    evidence: []const u8 = DEFAULT_EVIDENCE,
};

fn render(alloc: std.mem.Allocator, f: Fields) ![]u8 {
    return std.fmt.allocPrint(
        alloc,
        "{{\"repo\":\"{s}\",\"base_sha\":\"{s}\",\"files\":{s},\"diff\":\"{s}\",\"cause\":\"{s}\",\"evidence\":{s}}}",
        .{ f.repo, f.base_sha, f.files, f.diff, f.cause, f.evidence },
    );
}

/// Parse a rendered block. Caller deinits.
fn parseFields(alloc: std.mem.Allocator, f: Fields) !std.json.Parsed(repair_proposal.Proposal) {
    const raw = try render(alloc, f);
    defer alloc.free(raw);
    return repair_proposal.parse(alloc, raw);
}

fn hashOf(alloc: std.mem.Allocator, f: Fields) ![repair_proposal.HASH_HEX_LEN]u8 {
    const parsed = try parseFields(alloc, f);
    defer parsed.deinit();
    return repair_proposal.canonicalHashHex(parsed.value);
}

test "test_proposal_hash_canonical_and_immutable" {
    const alloc = testing.allocator;

    // Baseline, and the same proposal with its JSON keys in a different order:
    // the block is a record, not a sequence, so spelling must not matter.
    const baseline = try hashOf(alloc, .{});
    const reordered_keys_raw =
        \\{"cause":"the checkout handler dropped its error branch",
        \\ "evidence":[{"ref":"FROM logs-*","kind":"esql","digest":"sha256:0f3a"}],
        \\ "diff":"--- a/src/a.zig\n+++ b/src/a.zig\n@@ -1 +1 @@\n-old\n+new\n",
        \\ "files":["src/a.zig"],
        \\ "base_sha":"0123456789abcdef0123456789abcdef01234567",
        \\ "repo":"agentsfleet/agentsfleet"}
    ;
    const reordered = try repair_proposal.parse(alloc, reordered_keys_raw);
    defer reordered.deinit();
    try testing.expectEqualStrings(&baseline, &repair_proposal.canonicalHashHex(reordered.value));

    // The allowlist is a set: listing the same two files in the other order is
    // the same proposal, because parse sorts before anything hashes it.
    const forward = try hashOf(alloc, .{ .files = "[\"src/a.zig\",\"src/b.zig\"]" });
    const backward = try hashOf(alloc, .{ .files = "[\"src/b.zig\",\"src/a.zig\"]" });
    try testing.expectEqualStrings(&forward, &backward);

    // One byte of the diff changed is a different proposal — this is the
    // property that makes approval bind bytes instead of intentions.
    const mutated = try hashOf(alloc, .{
        .diff = "--- a/src/a.zig\\n+++ b/src/a.zig\\n@@ -1 +1 @@\\n-old\\n+neW\\n",
    });
    try testing.expect(!std.mem.eql(u8, &baseline, &mutated));

    // Length framing: the same bytes split differently across the allowlist
    // must not collide, or a path boundary could be forged.
    const split_left = try hashOf(alloc, .{ .files = "[\"ab\",\"c\"]" });
    const split_right = try hashOf(alloc, .{ .files = "[\"a\",\"bc\"]" });
    try testing.expect(!std.mem.eql(u8, &split_left, &split_right));

    // A different base is a different proposal even with an identical diff.
    const moved_base = try hashOf(alloc, .{ .base_sha = OTHER_SHA });
    try testing.expect(!std.mem.eql(u8, &baseline, &moved_base));

    // Cause and evidence are narrative, not payload: re-wording the
    // justification must not invalidate an approval of the same bytes.
    const reworded = try hashOf(alloc, .{ .cause = "the checkout handler swallowed an error" });
    try testing.expectEqualStrings(&baseline, &reworded);
}

test "test_malformed_proposal_is_rejected" {
    const alloc = testing.allocator;
    const cases = [_]struct { fields: Fields, want: anyerror }{
        .{ .fields = .{ .repo = "agentsfleet" }, .want = error.RepoShapeInvalid },
        .{ .fields = .{ .repo = "agentsfleet/deep/nested" }, .want = error.RepoShapeInvalid },
        .{ .fields = .{ .base_sha = "0123456" }, .want = error.BaseShaShapeInvalid },
        .{ .fields = .{ .base_sha = "0123456789ABCDEF0123456789abcdef01234567" }, .want = error.BaseShaShapeInvalid },
        .{ .fields = .{ .files = "[]" }, .want = error.FileListEmpty },
        .{ .fields = .{ .files = "[\"../../etc/passwd\"]" }, .want = error.FilePathUnsafe },
        .{ .fields = .{ .files = "[\"/etc/passwd\"]" }, .want = error.FilePathUnsafe },
        .{ .fields = .{ .files = "[\".git/config\"]" }, .want = error.FilePathUnsafe },
        .{ .fields = .{ .files = "[\"src//a.zig\"]" }, .want = error.FilePathUnsafe },
        .{ .fields = .{ .files = "[\"src/./a.zig\"]" }, .want = error.FilePathUnsafe },
        .{ .fields = .{ .diff = "" }, .want = error.DiffEmpty },
        .{ .fields = .{ .cause = "" }, .want = error.CauseEmpty },
        .{ .fields = .{ .evidence = "[]" }, .want = error.EvidenceMissing },
    };
    for (cases) |case| {
        try testing.expectError(case.want, parseFields(alloc, case.fields));
    }
}

test "test_oversized_proposal_is_rejected" {
    const alloc = testing.allocator;

    const huge_diff = try alloc.alloc(u8, repair_proposal.MAX_DIFF_BYTES + 1);
    defer alloc.free(huge_diff);
    @memset(huge_diff, 'x');
    try testing.expectError(error.DiffTooLarge, parseFields(alloc, .{ .diff = huge_diff }));

    var list: std.ArrayList(u8) = .empty;
    defer list.deinit(alloc);
    try list.append(alloc, '[');
    for (0..repair_proposal.MAX_FILES + 1) |i| {
        if (i > 0) try list.append(alloc, ',');
        try list.print(alloc, "\"src/f{d}.zig\"", .{i});
    }
    try list.append(alloc, ']');
    try testing.expectError(error.FileListTooLong, parseFields(alloc, .{ .files = list.items }));
}

test "test_proposal_survives_its_input_buffer" {
    // The report path parses a proposal out of a run-report body and is free
    // to release that body immediately. Every helper in this file already
    // frees the rendered bytes before asserting, so this states the invariant
    // outright: a parsed proposal owns its strings, and a parser that aliased
    // the input would hash whatever landed in the freed bytes next.
    const alloc = testing.allocator;
    const raw = try render(alloc, .{});
    const parsed = try repair_proposal.parse(alloc, raw);
    defer parsed.deinit();
    @memset(raw, 0);
    alloc.free(raw);

    try testing.expectEqualStrings("agentsfleet/agentsfleet", parsed.value.repo);
    try testing.expectEqualStrings(VALID_SHA, parsed.value.base_sha);
    try testing.expectEqualStrings("src/a.zig", parsed.value.files[0]);
}

test "test_repair_refusal_codes_are_registered" {
    // Every refusal an operator can meet resolves to a real registry entry
    // with real hint text — an unregistered code would show up in Slack and
    // the activity stream as an untraceable string.
    for (std.meta.tags(repair_proposal.Refusal)) |refusal| {
        const code = refusal.code();
        try testing.expect(code.len > 0);
        const entry = ec.lookup(code);
        try testing.expect(!std.mem.eql(u8, entry.code, ec.UNKNOWN.code));
        try testing.expect(entry.hint.len > 0);
    }
}

test "test_stale_base_is_detected" {
    const alloc = testing.allocator;
    const parsed = try parseFields(alloc, .{});
    defer parsed.deinit();
    try testing.expect(repair_proposal.baseIsFresh(parsed.value, VALID_SHA));
    try testing.expect(!repair_proposal.baseIsFresh(parsed.value, OTHER_SHA));
}

test "test_branch_name_is_derived_from_proposal_id" {
    const first_id = "01930000-0000-7000-8000-0000000000aa";
    const second_id = "01930000-0000-7000-8000-0000000000bb";

    var buf_a: [repair_proposal.BRANCH_NAME_MAX]u8 = undefined;
    var buf_b: [repair_proposal.BRANCH_NAME_MAX]u8 = undefined;
    var buf_c: [repair_proposal.BRANCH_NAME_MAX]u8 = undefined;

    // Same identifier, same branch — this is what makes a replayed approval
    // land on the branch that already exists instead of a second one.
    const once = try repair_proposal.branchName(&buf_a, first_id);
    const twice = try repair_proposal.branchName(&buf_b, first_id);
    try testing.expectEqualStrings(once, twice);

    const other = try repair_proposal.branchName(&buf_c, second_id);
    try testing.expect(!std.mem.eql(u8, once, other));
    try testing.expect(std.mem.endsWith(u8, once, first_id));
}

test "test_proposal_block_kind_is_pinned" {
    // pin test: literal is the contract
    try testing.expectEqualStrings("repair_proposal/1", repair_proposal.BLOCK_KIND);
}
