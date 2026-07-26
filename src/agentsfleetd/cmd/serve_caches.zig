//! Process-lifetime caches the API server owns.
//!
//! `serve.zig` sits within a few lines of its 350-line cap, so the wiring for
//! anything with a construct/teardown pair lives beside it rather than in it —
//! the same seam `serve_r2.zig`, `serve_secrets.zig` and `serve_background.zig`
//! already occupy. Every cache added here costs `serve.zig` no more lines than
//! the first one did.
//!
//! Storage is file-global rather than returned by value because these are
//! genuinely process singletons: one catalogue cache serves every request, and
//! the `Context` holds a borrowed pointer to it for the server's whole life.
//! `init` is therefore called exactly once, from `serve.run`.

const std = @import("std");

const model_library_cache = @import("../state/model_library_cache.zig");

/// The §2 catalogue response cache. Storage lives here for the process lifetime;
/// `Context.model_library_cache` borrows a pointer to it.
// SAFETY: guarded by `g_ready`, which starts false — nothing reads this storage
// until `init` has written a real cache into it and set the flag.
var g_model_library: model_library_cache.Cache = undefined;
var g_ready: bool = false;

/// Construct the caches and hand back the catalogue one for `Context` to borrow.
///
/// Idempotent: a second call returns the existing instance rather than leaking
/// the first. `serve.run` is the only caller, but a re-entered boot (a test that
/// starts the server twice in one process) must not silently strand a cache
/// holding every payload it had admitted.
pub fn init(alloc: std.mem.Allocator) !*model_library_cache.Cache {
    if (!g_ready) {
        g_model_library = try model_library_cache.Cache.init(alloc);
        g_ready = true;
    }
    return &g_model_library;
}

/// Tear down every cache. Safe to call when `init` never ran, so `serve.run` can
/// defer it before the boot steps that might fail.
pub fn deinit() void {
    if (!g_ready) return;
    g_model_library.deinit();
    g_ready = false;
}

// ── tests ───────────────────────────────────────────────────────────────────

test "init is idempotent and deinit tolerates never having run" {
    // The re-entered-boot case: the second init must hand back the SAME cache,
    // because a fresh one would strand every payload the first had admitted.
    deinit(); // tolerate a prior test having initialized it
    const a = try init(std.testing.allocator);
    const b = try init(std.testing.allocator);
    try std.testing.expectEqual(a, b);

    deinit();
    deinit(); // second teardown is a no-op, not a double free
}
