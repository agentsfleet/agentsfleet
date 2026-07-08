// Repo-wide sweep for internalOperationError() call sites
// passing jargon into the wire-visible `detail` field.
//
// `common.internalOperationError(res, detail, req_id)` always resolves to
// UZ-INTERNAL-003 (error_entries.zig), which is e()-only — no curated
// user_message. The dashboard client prefers `user_message` but falls back
// to the raw `detail` string when absent (lib/api/client.ts: `user_message
// ?? detail ?? title`), so every one of these call sites' literal `detail`
// argument reaches the dashboard verbatim whenever the route is
// dashboard-reachable. A prior curation pass fixed the one concretely-reported
// offender (tenant_provider.zig's platform-key-missing path, now its own
// curated UZ-PROVIDER-009); a follow-up sweep found 86 call sites total (50
// plain English, 35 jargon-leaking, 1 raw-tag-leaking in http/server.zig) but
// fixed none of them, pinning the count instead.
//
// A de-mudball pass then closed that backlog: server.zig's raw @errorName leak
// now maps to a stable detail (never the raw tag); of the 35 jargon sites, 5
// call sites were promoted to their own eu()/e() registry codes (UZ-CONN-007
// x2, UZ-AGT-013, UZ-CRED-002, UZ-PROVIDER-010) and now call
// common.errorResponse() directly instead of internalOperationError() — hence
// the baseline below dropped by 5 (86 -> 81); the rest were scrubbed in place
// to plain-English details.
//
// identity_events_clerk.zig's Clerk-webhook-secret-missing/empty path (2 call
// sites) was deliberately NOT promoted to its own code, unlike the other
// jargon sites: `/review` caught that a distinct code/title there would
// confirm to an unauthenticated caller that this deployment has no
// CLERK_WEBHOOK_SECRET configured — exactly what that file's own header
// comment says must not happen. Those 2 sites stay on the generic
// UZ-INTERNAL-003 catch-all with a scrubbed detail, justified via
// `// mudball-ok:` rather than counted against BASELINE (81 - 2 justified =
// 79, unchanged from the pre-revert count).
//
// This test still only pins the count: a new call site under
// http/handlers/** forces whoever adds it to consciously bump BASELINE
// below, at which point they should ask whether the new `detail` string
// reads like plain English to a dashboard end user or needs its own
// eu()-curated registry code instead of piggybacking on UZ-INTERNAL-003.
//
// M121 §2 added 2 call sites in handlers/tenant_model_entries.zig ("Failed to
// build the models list", "Failed to mint an entry id") — both plain English,
// no jargon/schema names/@errorName, so bumped straight into BASELINE rather
// than mudball-ok'd (79 -> 81).
const std = @import("std");
const common = @import("common");

const BASELINE_CALL_SITE_COUNT: usize = 81;
const CALL_SITE_NEEDLE = "internalOperationError(";
const HANDLERS_DIR_PATH = "src/agentsfleetd/http/handlers";
// `http/server.zig` sits one level above `http/handlers/` (the dispatcher,
// not a handler) but has its own call site — the sweep's single worst
// finding, `@errorName(e)` leaking a raw Zig error-union tag on every
// authenticated route's middleware-failure path. A walk scoped to
// `http/handlers/` alone would silently exclude it from this tripwire.
const EXTRA_FILES = [_][]const u8{"src/agentsfleetd/http/server.zig"};

// Justify-per-add. A call site counts against the frozen BASELINE unless it
// carries an inline `// mudball-ok: <reason>` comment (same line or the line
// immediately above, matching server.zig's own internalOperationError() call
// in its auth-mw failure path) — so a new site can be added WITHOUT bumping
// BASELINE, but only by explicitly arguing why it's not a mudball recurrence;
// an unjustified add still fails exactly as before.
const MUDBALL_OK_MARKER = "mudball-ok";

fn lineContaining(content: []const u8, pos: usize) []const u8 {
    const start = if (std.mem.lastIndexOfScalar(u8, content[0..pos], '\n')) |i| i + 1 else 0;
    const end = std.mem.indexOfScalarPos(u8, content, pos, '\n') orelse content.len;
    return content[start..end];
}

fn isJustified(content: []const u8, call_pos: usize) bool {
    const this_line = lineContaining(content, call_pos);
    if (std.mem.indexOf(u8, this_line, MUDBALL_OK_MARKER) != null) return true;
    const line_start = std.mem.lastIndexOfScalar(u8, content[0..call_pos], '\n') orelse return false;
    if (line_start == 0) return false;
    const prev_line = lineContaining(content, line_start - 1);
    return std.mem.indexOf(u8, prev_line, MUDBALL_OK_MARKER) != null;
}

/// Returns (total call sites, justified call sites) for one file's content.
pub fn countCallSites(content: []const u8) struct { total: usize, justified: usize } {
    var total: usize = 0;
    var justified: usize = 0;
    var idx: usize = 0;
    while (std.mem.indexOfPos(u8, content, idx, CALL_SITE_NEEDLE)) |pos| {
        total += 1;
        if (isJustified(content, pos)) justified += 1;
        idx = pos + CALL_SITE_NEEDLE.len;
    }
    return .{ .total = total, .justified = justified };
}

test "internalOperationError() call sites under http/handlers/ (+ http/server.zig) don't grow past the sweep baseline" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();

    // Tests run from the repo root (zig build sets cwd) — same convention as
    // auth/scopes.zig's docs/AUTH.md parity test and
    // http/handlers/tenant_provider_dispatch_test.zig's own file read.
    var handlers_dir = try std.Io.Dir.cwd().openDir(io, HANDLERS_DIR_PATH, .{ .iterate = true });
    defer handlers_dir.close(io);

    var walker = try handlers_dir.walk(alloc);
    defer walker.deinit();

    var total: usize = 0;
    var justified: usize = 0;
    while (try walker.next(io)) |entry| {
        if (entry.kind != .file) continue;
        if (!std.mem.endsWith(u8, entry.basename, ".zig")) continue;
        if (std.mem.endsWith(u8, entry.basename, "_test.zig")) continue;
        // common.zig only defines internalOperationError(); it never calls it.
        if (std.mem.eql(u8, entry.basename, "common.zig")) continue;

        const content = try entry.dir.readFileAlloc(io, entry.basename, alloc, .limited(256 * 1024));
        defer alloc.free(content);

        const counted = countCallSites(content);
        total += counted.total;
        justified += counted.justified;
    }

    for (EXTRA_FILES) |path| {
        const content = try std.Io.Dir.cwd().readFileAlloc(io, path, alloc, .limited(256 * 1024));
        defer alloc.free(content);
        const counted = countCallSites(content);
        total += counted.total;
        justified += counted.justified;
    }

    const unjustified = total - justified;
    if (unjustified > BASELINE_CALL_SITE_COUNT) {
        std.debug.print(
            "internalOperationError() call sites grew from {d} to {d} unjustified (justified via `// mudball-ok:`: {d}) without an accounted sweep update.\n" ++
                "New call site(s) must be triaged: does the detail string read like plain English to a\n" ++
                "dashboard end user, or does it leak internal jargon (schema/table names, alloc/OOM,\n" ++
                "state-machine language, an @errorName(err) variable)? If jargon, prefer a dedicated\n" ++
                "eu()-curated registry code (see error_entries.zig's module doc) over piggybacking on\n" ++
                "UZ-INTERNAL-003. Either way, add an inline `// mudball-ok: <reason>` to justify it, or\n" ++
                "bump BASELINE_CALL_SITE_COUNT in this test.\n",
            .{ BASELINE_CALL_SITE_COUNT, unjustified, justified },
        );
        return error.TestUnexpectedResult;
    }
}

// None of the surviving internalOperationError()
// call sites may pass a jargon detail (component/schema names, alloc/OOM,
// state-machine phrasing, a raw @errorName variable — quoted or not: a bare
// `@errorName(e)` expression, the original server.zig bug, carries no quotes
// at all). Scoped strictly to each call's own argument text — found by
// tracking paren depth (string-literal-aware) from the needle's `(` to its
// MATCHING `)`, never a fixed `");"` suffix: a call site inside a switch arm
// closes with `),` not `);`, and a naive suffix search would silently scan
// past it into unrelated later code.
pub const JARGON_DENYLIST = [_][]const u8{
    "@errorName",       "alloc",       "OOM",   "serialization",
    "canonicalization", "idempotency", "dedup", "invariant",
    "bootstrap",
};

/// Returns the index of the `)` matching the `(` at `open_paren_pos`.
pub fn matchingCloseParen(content: []const u8, open_paren_pos: usize) ?usize {
    var depth: usize = 1;
    var in_string = false;
    var i = open_paren_pos + 1;
    while (i < content.len) : (i += 1) {
        const c = content[i];
        if (in_string) {
            if (c == '\\') {
                i += 1;
            } else if (c == '"') {
                in_string = false;
            }
            continue;
        }
        switch (c) {
            '"' => in_string = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if (depth == 0) return i;
            },
            else => {},
        }
    }
    return null;
}

pub fn scanCallSitesForJargon(path: []const u8, content: []const u8) !void {
    var idx: usize = 0;
    while (std.mem.indexOfPos(u8, content, idx, CALL_SITE_NEEDLE)) |pos| {
        const open_paren_pos = pos + CALL_SITE_NEEDLE.len - 1;
        const call_end = matchingCloseParen(content, open_paren_pos) orelse content.len;
        idx = call_end + 1;
        const args = content[open_paren_pos + 1 .. call_end];

        for (JARGON_DENYLIST) |token| {
            if (std.mem.indexOf(u8, args, token) != null) {
                std.debug.print("jargon token \"{s}\" found in an internalOperationError() call's args in {s}: \"{s}\"\n", .{ token, path, args });
                return error.TestUnexpectedResult;
            }
        }
    }
}

test "no surviving internalOperationError() detail leaks a jargon token" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();

    var handlers_dir = try std.Io.Dir.cwd().openDir(io, HANDLERS_DIR_PATH, .{ .iterate = true });
    defer handlers_dir.close(io);

    var walker = try handlers_dir.walk(alloc);
    defer walker.deinit();

    while (try walker.next(io)) |entry| {
        if (entry.kind != .file) continue;
        if (!std.mem.endsWith(u8, entry.basename, ".zig")) continue;
        if (std.mem.endsWith(u8, entry.basename, "_test.zig")) continue;
        if (std.mem.eql(u8, entry.basename, "common.zig")) continue;

        const content = try entry.dir.readFileAlloc(io, entry.basename, alloc, .limited(256 * 1024));
        defer alloc.free(content);

        try scanCallSitesForJargon(entry.basename, content);
    }

    for (EXTRA_FILES) |path| {
        const content = try std.Io.Dir.cwd().readFileAlloc(io, path, alloc, .limited(256 * 1024));
        defer alloc.free(content);
        try scanCallSitesForJargon(path, content);
    }
}
