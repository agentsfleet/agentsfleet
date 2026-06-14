// Source-level guard for the redaction rule in sessions.zig (Invariant 16):
// every `.auth`-scope `log.*` emit carrying `session_id` must route it
// through `helpers.redactSid`. The raw session_id is capability-bearing
// (it rides in the verification URL; combined with the 6-digit code it
// authorizes ciphertext release), so a plaintext id on a log line is a
// credential leak.
//
// A unit test on the redact fn (audit.zig) proves the function works but
// can't catch a *caller* that binds the raw id and bypasses it — which is
// exactly the regression this scans the handler source for. The legitimate
// raw `.session_id = session_id` binding in the `hx.ok` response body is
// out of a log-emit span and so is correctly ignored.
//
// Tests run from the repo root (zig build sets cwd); the path is relative
// to the project root, mirroring frontmatter_fixtures_test.zig.

const std = @import("std");
const common = @import("common");

const SESSIONS_SRC = "src/agentsfleetd/http/handlers/auth/sessions.zig";

const LOG_EMIT_MARKERS = [_][]const u8{ "log.info(", "log.warn(", "log.err(", "log.debug(" };

fn opensLogEmit(line: []const u8) bool {
    for (LOG_EMIT_MARKERS) |m| {
        if (std.mem.indexOf(u8, line, m) != null) return true;
    }
    return false;
}

// Walks the file line by line, tracking whether the cursor is inside a
// `log.*` emit (from its opener to the closing `});`). A `.session_id =`
// field seen inside that span must also name `redactSid`. Returns the count
// of redacted session_id log bindings so the caller can reject a vacuous
// pass (path or scanner silently broke).
fn countRedactedSidLogBindings(src: []const u8) !usize {
    var in_emit = false;
    var redacted: usize = 0;
    var it = std.mem.splitScalar(u8, src, '\n');
    while (it.next()) |line| {
        if (opensLogEmit(line)) in_emit = true;
        if (in_emit and std.mem.indexOf(u8, line, ".session_id =") != null) {
            if (std.mem.indexOf(u8, line, "redactSid") == null) return error.RawSessionIdInLogEmit;
            redacted += 1;
        }
        if (std.mem.indexOf(u8, line, "});") != null) in_emit = false;
    }
    return redacted;
}

test "sessions.zig routes session_id through redactSid in every .auth log emit" {
    const alloc = std.testing.allocator;
    const src = try std.Io.Dir.cwd().readFileAlloc(common.globalIo(), SESSIONS_SRC, alloc, .limited(256 * 1024));
    defer alloc.free(src);

    const redacted = try countRedactedSidLogBindings(src);
    // Anti-vacuous floor: the handler redacts at least the create,
    // approve-success, and approve-audit-lookup-fail emits. A drop to zero
    // means the scanner or the source path silently broke.
    try std.testing.expect(redacted >= 1);
}

test "the redaction scanner rejects a raw session_id binding in a log emit" {
    // Self-test the scanner against a minimal positive (redacted) and
    // negative (raw) fixture so a future no-op refactor of the scanner is
    // caught, not just a regression in the handler.
    const safe =
        \\log.warn("evt", .{
        \\    .session_id = helpers.redactSid(&buf, session_id),
        \\});
    ;
    try std.testing.expect((try countRedactedSidLogBindings(safe)) == 1);

    const leaky =
        \\log.warn("evt", .{
        \\    .session_id = session_id,
        \\});
    ;
    try std.testing.expectError(error.RawSessionIdInLogEmit, countRedactedSidLogBindings(leaky));
}
