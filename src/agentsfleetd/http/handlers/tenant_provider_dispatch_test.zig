// Gap-fill — Dimension 8.1/8.2 coverage audit found no test
// pinning tenant_provider.zig's applyPlatform dispatch itself: a live-DB
// integration test that forces PlatformKeyMissing needs a globally-empty
// core.platform_llm_keys, which races every other integration test's
// seeding on the shared pool (same reasoning tenant_provider_test.zig's
// "PlatformKeyMissing path is exercised in..." comment documents for the
// state-layer test — see that file, ~line 254). applyPlatform itself is
// file-private (not pub), so it cannot be unit-tested directly without a
// live `*pg.Conn` either (upsertPlatform needs one).
//
// This is a text-contract pin instead: it reads the handler source and
// asserts the PlatformKeyMissing arm still dispatches through the curated
// registry code, not the raw internalOperationError(...) string literal
// path this spec's Dimension 8.1 replaced. A regression back to the old
// path (or a copy-paste onto the wrong ERR_* constant) fails this test
// without needing a live database.
const std = @import("std");
const common = @import("common");

// Read cap for slurping the handler source into memory — tenant_provider.zig is
// a few KiB; 64 KiB is generous headroom without an unbounded allocation.
const MAX_SOURCE_BYTES = 64 * 1024;

test "tenant_provider.zig's PlatformKeyMissing arm dispatches through ERR_PROVIDER_PLATFORM_KEY_MISSING, not a raw internalOperationError literal" {
    const alloc = std.testing.allocator;
    // Tests run from the repo root (zig build sets cwd) — same convention as
    // auth/scopes.zig's docs/AUTH.md parity test and
    // fleet_runtime/frontmatter_fixtures_test.zig's fixture reads.
    const src = try std.Io.Dir.cwd().readFileAlloc(
        common.globalIo(),
        "src/agentsfleetd/http/handlers/tenant_provider.zig",
        alloc,
        .limited(MAX_SOURCE_BYTES),
    );
    defer alloc.free(src);

    const arm_start = std.mem.indexOf(u8, src, "PlatformKeyMissing =>") orelse {
        std.debug.print("tenant_provider.zig: PlatformKeyMissing arm not found\n", .{});
        return error.TestUnexpectedResult;
    };
    // The arm is a short `{ ... }` block; the next "},\n" after it closes it.
    const arm_end_rel = std.mem.indexOf(u8, src[arm_start..], "},\n") orelse src.len - arm_start;
    const arm = src[arm_start .. arm_start + arm_end_rel];

    if (std.mem.indexOf(u8, arm, "ERR_PROVIDER_PLATFORM_KEY_MISSING") == null) {
        std.debug.print("PlatformKeyMissing arm no longer references ERR_PROVIDER_PLATFORM_KEY_MISSING:\n{s}\n", .{arm});
        return error.TestUnexpectedResult;
    }
    if (std.mem.indexOf(u8, arm, "internalOperationError") != null) {
        std.debug.print("PlatformKeyMissing arm regressed to the raw internalOperationError(...) path:\n{s}\n", .{arm});
        return error.TestUnexpectedResult;
    }
    // The exact jargon string the curation removed from this toast.
    if (std.mem.indexOf(u8, arm, "operator action required") != null) {
        std.debug.print("PlatformKeyMissing arm still leaks operator jargon:\n{s}\n", .{arm});
        return error.TestUnexpectedResult;
    }
}
