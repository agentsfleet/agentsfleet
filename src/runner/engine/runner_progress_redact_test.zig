//! Contract tests for `runner_progress.redactBytes` — the byte-level
//! primitive every wire-redaction call site eventually delegates to.
//!
//! The redactor's invariants live here as a table-driven test: each row
//! pairs an input with the expected output and a one-line rationale that
//! doubles as the spec. Reviewers read the table to know what the redactor
//! promises; a regression that breaks any row fails loudly at unit-test
//! speed instead of surfacing through the integration pipeline.
//!
//! Where a row is marked `DECISION POINT` the rationale records which way
//! the current implementation goes — the test pins that behaviour. A
//! future intentional change must update both the table and the rationale,
//! surfacing the change at review time.
//!
//! Coverage axes:
//!   1. Byte placement   — start, middle, end, JSON-quote boundaries
//!   2. Substring shape  — exact, overlap, secret-as-prefix-of-other
//!   3. Multiplicity     — single, repeated, multiple distinct secrets
//!   4. Empty / boundary — empty input, empty value, empty placeholder
//!   5. Encoding         — multi-byte UTF-8, NUL-byte safety
//!
//! Companion to the integration tests in
//! `src/zombie/event_loop_harness_redaction_test.zig`. This file isolates
//! the primitive so a contract regression need not surface through the
//! pipeline to be caught.

const std = @import("std");
const runner_progress = @import("runner_progress.zig");
const Secret = runner_progress.Secret;

const PH = "${secrets.llm.api_key}";
const PH_GH = "${secrets.github.token}";

const Case = struct {
    name: []const u8,
    input: []const u8,
    secrets: []const Secret,
    expected: []const u8,
    rationale: []const u8,
};

fn runCase(case: Case) !void {
    const alloc = std.testing.allocator;
    const out = try runner_progress.redactBytes(alloc, case.input, case.secrets);
    defer if (out.ptr != case.input.ptr) alloc.free(out);
    std.testing.expectEqualStrings(case.expected, out) catch |err| {
        std.debug.print(
            "\nFAILED: {s}\n  rationale: {s}\n  expected:  '{s}'\n  got:       '{s}'\n",
            .{ case.name, case.rationale, case.expected, out },
        );
        return err;
    };
}

// ── Byte placement ──────────────────────────────────────────────────────────

test "redactBytes: secret at start of input is replaced" {
    try runCase(.{
        .name = "start-of-input",
        .input = "sk-abc remainder",
        .secrets = &.{.{ .value = "sk-abc", .placeholder = PH }},
        .expected = PH ++ " remainder",
        .rationale = "Secret at offset 0 — replacement must not require leading context.",
    });
}

test "redactBytes: secret at end of input is replaced" {
    try runCase(.{
        .name = "end-of-input",
        .input = "prefix sk-abc",
        .secrets = &.{.{ .value = "sk-abc", .placeholder = PH }},
        .expected = "prefix " ++ PH,
        .rationale = "Secret bumping the right boundary — no trailing context required.",
    });
}

test "redactBytes: secret across JSON quote boundary substitutes inside the string" {
    try runCase(.{
        .name = "json-quote-boundary",
        .input = "{\"k\":\"sk-abc\"}",
        .secrets = &.{.{ .value = "sk-abc", .placeholder = PH }},
        .expected = "{\"k\":\"" ++ PH ++ "\"}",
        .rationale = "JSON quotes are bytes; the redactor doesn't parse JSON. Surrounding quotes are preserved verbatim.",
    });
}

test "redactBytes: secret in JSON object key is replaced (not just values)" {
    try runCase(.{
        .name = "json-object-key",
        .input = "{\"sk-abc\":\"value\"}",
        .secrets = &.{.{ .value = "sk-abc", .placeholder = PH }},
        .expected = "{\"" ++ PH ++ "\":\"value\"}",
        .rationale = "DECISION POINT: bytes-only — keys scrub identically to values. JSON-aware logic would require replacing the byte loop.",
    });
}

// ── Substring shape ─────────────────────────────────────────────────────────

test "redactBytes: substring overlap with a benign longer string is greedy-replaced" {
    try runCase(.{
        .name = "substring-overlap",
        .input = "abc abcdef",
        .secrets = &.{.{ .value = "abc", .placeholder = "<X>" }},
        .expected = "<X> <X>def",
        .rationale = "DECISION POINT: redactor matches bytes greedily, no word-boundary check. Real high-entropy secrets won't collide; flagged so a low-entropy 'secret' is anticipated.",
    });
}

test "redactBytes: secret as prefix of a different secret — first listed wins" {
    try runCase(.{
        .name = "secret-prefix-ordering",
        .input = "abcd",
        .secrets = &.{
            .{ .value = "abc", .placeholder = "<SHORT>" },
            .{ .value = "abcd", .placeholder = "<LONG>" },
        },
        .expected = "<SHORT>d",
        .rationale = "DECISION POINT: secrets process in iteration order. Caller's responsibility to ensure no real secret is a prefix of another.",
    });
}

test "redactBytes: identical placeholders for distinct secrets is allowed" {
    try runCase(.{
        .name = "shared-placeholder",
        .input = "alpha beta",
        .secrets = &.{
            .{ .value = "alpha", .placeholder = "<X>" },
            .{ .value = "beta", .placeholder = "<X>" },
        },
        .expected = "<X> <X>",
        .rationale = "Same placeholder for two distinct slots — valid; pinned for future use.",
    });
}

// ── Multiplicity ────────────────────────────────────────────────────────────

test "redactBytes: every occurrence of a secret is replaced (not just the first)" {
    try runCase(.{
        .name = "all-occurrences",
        .input = "sk-abc and sk-abc and sk-abc",
        .secrets = &.{.{ .value = "sk-abc", .placeholder = PH }},
        .expected = PH ++ " and " ++ PH ++ " and " ++ PH,
        .rationale = "Single secret, multiple hits — std.mem.replace is global. A single-replacement bug here would leak repeated occurrences.",
    });
}

test "redactBytes: two adjacent occurrences with no separator both replace" {
    try runCase(.{
        .name = "adjacent-occurrences",
        .input = "sk-abcsk-abc",
        .secrets = &.{.{ .value = "sk-abc", .placeholder = "<X>" }},
        .expected = "<X><X>",
        .rationale = "Adjacent runs of the same secret — replacement loop must advance past placeholder, not re-scan from match start.",
    });
}

test "redactBytes: multiple distinct secrets in same input both replace" {
    try runCase(.{
        .name = "multi-secret",
        .input = "{\"key\":\"sk-abc\",\"token\":\"ghp-xyz\"}",
        .secrets = &.{
            .{ .value = "sk-abc", .placeholder = PH },
            .{ .value = "ghp-xyz", .placeholder = PH_GH },
        },
        .expected = "{\"key\":\"" ++ PH ++ "\",\"token\":\"" ++ PH_GH ++ "\"}",
        .rationale = "Mirrors the two-slot config. A break-after-first-match bug fails this row.",
    });
}

// ── Empty / boundary ────────────────────────────────────────────────────────

test "redactBytes: empty input returns empty output regardless of secrets" {
    try runCase(.{
        .name = "empty-input",
        .input = "",
        .secrets = &.{.{ .value = "sk-abc", .placeholder = PH }},
        .expected = "",
        .rationale = "Empty input is the no-allocation fast path. Crash-on-empty bug would surface here.",
    });
}

test "redactBytes: empty secret value is skipped (does not match anywhere)" {
    try runCase(.{
        .name = "empty-secret-value",
        .input = "anything goes",
        .secrets = &.{.{ .value = "", .placeholder = PH }},
        .expected = "anything goes",
        .rationale = "DECISION POINT: empty value = unconfigured slot. 'Matches everything' would scrub the entire input — disastrous; skip is the only safe choice.",
    });
}

test "redactBytes: empty secrets list is a no-op (input pointer returned as-is)" {
    const alloc = std.testing.allocator;
    const input = "no scrubbing today";
    const out = try runner_progress.redactBytes(alloc, input, &.{});
    defer if (out.ptr != input.ptr) alloc.free(out);
    try std.testing.expectEqual(input.ptr, out.ptr);
    try std.testing.expectEqualStrings(input, out);
}

test "redactBytes: no-match returns input pointer (caller skips free)" {
    const alloc = std.testing.allocator;
    const input = "{\"safe\":\"contents\"}";
    const secrets = [_]Secret{.{ .value = "sk-absent", .placeholder = PH }};
    const out = try runner_progress.redactBytes(alloc, input, &secrets);
    defer if (out.ptr != input.ptr) alloc.free(out);
    try std.testing.expectEqual(input.ptr, out.ptr);
    try std.testing.expectEqualStrings(input, out);
}

test "redactBytes: empty placeholder deletes the secret bytes" {
    try runCase(.{
        .name = "empty-placeholder",
        .input = "before sk-abc after",
        .secrets = &.{.{ .value = "sk-abc", .placeholder = "" }},
        .expected = "before  after",
        .rationale = "DECISION POINT: empty placeholder = byte deletion. Surrounding whitespace is preserved verbatim — no coalescing.",
    });
}

// ── Encoding / binary safety ────────────────────────────────────────────────

test "redactBytes: multi-byte UTF-8 secret matches byte-for-byte" {
    try runCase(.{
        .name = "utf8-secret",
        .input = "hello héllo world",
        .secrets = &.{.{ .value = "héllo", .placeholder = "<X>" }},
        .expected = "hello <X> world",
        .rationale = "Secrets are bytes, not codepoints. A multi-byte secret is matched as the underlying UTF-8 byte sequence.",
    });
}

test "redactBytes: secret containing a NUL byte still matches" {
    try runCase(.{
        .name = "nul-byte-secret",
        .input = "left\x00mid\x00right",
        .secrets = &.{.{ .value = "\x00mid\x00", .placeholder = "<X>" }},
        .expected = "left<X>right",
        .rationale = "Inputs are []u8 slices; std.mem.indexOf handles NUL inside the needle. Defensive — binary tool output containing a NUL would otherwise truncate.",
    });
}
