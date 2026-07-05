// Repo-wide sweep for internalOperationError() call sites
// passing jargon into the wire-visible `detail` field.
//
// `common.internalOperationError(res, detail, req_id)` always resolves to
// UZ-INTERNAL-003 (error_entries.zig), which is e()-only — no curated
// user_message. The dashboard client prefers `user_message` but falls back
// to the raw `detail` string when absent (lib/api/client.ts: `user_message
// ?? detail ?? title`), so every one of these call sites' literal `detail`
// argument reaches the dashboard verbatim whenever the route is
// dashboard-reachable. Dimension 8.1 fixed the one concretely-reported
// offender (tenant_provider.zig's platform-key-missing path, now its own
// curated UZ-PROVIDER-009). A full sweep this milestone found 86 call
// sites total under http/handlers/**: 50 read as plain English ("Failed to
// create workspace"), 35 leak internal jargon (schema/component names,
// "alloc"/"OOM", state-machine language like "bootstrap invariant
// violated"), and 1 (http/server.zig) leaks a raw `@errorName(err)` Zig
// error-union tag instead of a literal at all.
//
// None of the 35/1 are fixed here — curating each would mean minting ~35
// new registry codes, well beyond this milestone's scope (see the spec's
// Out of Scope: "tracked as a further follow-up if they surface as real
// complaints"). This test only pins the count: a new call site under
// http/handlers/** forces whoever adds it to consciously bump BASELINE
// below, at which point they should ask whether the new `detail` string
// reads like plain English to a dashboard end user or needs its own
// eu()-curated registry code instead of piggybacking on UZ-INTERNAL-003.
const std = @import("std");
const common = @import("common");

const BASELINE_CALL_SITE_COUNT: usize = 86;
const CALL_SITE_NEEDLE = "internalOperationError(";
const HANDLERS_DIR_PATH = "src/agentsfleetd/http/handlers";
// `http/server.zig` sits one level above `http/handlers/` (the dispatcher,
// not a handler) but has its own call site — the sweep's single worst
// finding, `@errorName(e)` leaking a raw Zig error-union tag on every
// authenticated route's middleware-failure path. A walk scoped to
// `http/handlers/` alone would silently exclude it from this tripwire.
const EXTRA_FILES = [_][]const u8{"src/agentsfleetd/http/server.zig"};

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
    while (try walker.next(io)) |entry| {
        if (entry.kind != .file) continue;
        if (!std.mem.endsWith(u8, entry.basename, ".zig")) continue;
        if (std.mem.endsWith(u8, entry.basename, "_test.zig")) continue;
        // common.zig only defines internalOperationError(); it never calls it.
        if (std.mem.eql(u8, entry.basename, "common.zig")) continue;

        const content = try entry.dir.readFileAlloc(io, entry.basename, alloc, .limited(256 * 1024));
        defer alloc.free(content);

        total += std.mem.count(u8, content, CALL_SITE_NEEDLE);
    }

    for (EXTRA_FILES) |path| {
        const content = try std.Io.Dir.cwd().readFileAlloc(io, path, alloc, .limited(256 * 1024));
        defer alloc.free(content);
        total += std.mem.count(u8, content, CALL_SITE_NEEDLE);
    }

    if (total > BASELINE_CALL_SITE_COUNT) {
        std.debug.print(
            "internalOperationError() call sites grew from {d} to {d} without an accounted sweep update.\n" ++
                "New call site(s) must be triaged: does the detail string read like plain English to a\n" ++
                "dashboard end user, or does it leak internal jargon (schema/table names, alloc/OOM,\n" ++
                "state-machine language, an @errorName(err) variable)? If jargon, prefer a dedicated\n" ++
                "eu()-curated registry code (see error_entries.zig's module doc) over piggybacking on\n" ++
                "UZ-INTERNAL-003. Either way, bump BASELINE_CALL_SITE_COUNT in this test.\n",
            .{ BASELINE_CALL_SITE_COUNT, total },
        );
        return error.TestUnexpectedResult;
    }
}
