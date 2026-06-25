//! Streaming secret redaction across `StreamChunk` boundaries.
//!
//! `runner_progress.redactBytes` redacts one whole buffer. The live tail
//! arrives as many small deltas, and a secret value can split across two of
//! them (`"sk-ab"` then `"c123"`): redacting each delta on its own misses the
//! seam and streams the raw fragments to the parent. `push` closes that seam —
//! it carries a trailing slice (up to the longest secret minus one byte) into
//! the next delta, redacts the joined buffer, and emits only the prefix that no
//! future delta can still turn into a secret. The held tail is dropped at
//! stream end (the durable final reply, redacted whole, carries the complete
//! content), so a partial-secret head is never emitted on the wire (M100 §1).

const std = @import("std");
const Allocator = std.mem.Allocator;
const runner_progress = @import("runner_progress.zig");
const Secret = runner_progress.Secret;

/// Append `delta`, redact across the carried boundary, and return the bytes
/// safe to emit now — caller owns and frees them; an empty slice means nothing
/// is emittable yet. `carry` holds the raw, un-emitted tail between calls; the
/// caller owns it and must `carry.deinit(alloc)` when the stream ends. On OOM
/// the carry is left exactly as it was (the chunk is dropped fail-closed by the
/// caller) and the error is returned.
pub fn push(
    alloc: Allocator,
    carry: *std.ArrayListUnmanaged(u8),
    delta: []const u8,
    secrets: []const Secret,
) ![]u8 {
    // Build `carry ++ delta` in a scratch buffer so a failure leaves `carry`
    // untouched — no half-applied delta to double-count on the next call.
    var work: std.ArrayListUnmanaged(u8) = .empty;
    defer work.deinit(alloc);
    try work.appendSlice(alloc, carry.items);
    try work.appendSlice(alloc, delta);

    const red = try runner_progress.redactBytes(alloc, work.items, secrets);
    const aliased = red.ptr == work.items.ptr; // redactBytes returns input when no hit
    defer if (!aliased) alloc.free(red);

    // Hold back the longest suffix that could still be the head of a secret a
    // later delta completes; emit everything before it.
    const hold = pendingPrefixLen(red, secrets);
    const emit_len = red.len - hold;

    const emit = try alloc.dupe(u8, red[0..emit_len]);
    errdefer alloc.free(emit);

    // Commit the held tail as the new carry only after every alloc succeeded,
    // so an OOM above never mutates the caller's carry.
    var next: std.ArrayListUnmanaged(u8) = .empty;
    errdefer next.deinit(alloc);
    try next.appendSlice(alloc, red[emit_len..]);
    carry.deinit(alloc);
    carry.* = next;

    return emit;
}

/// Longest suffix of `buf` that equals the leading bytes of some secret value,
/// capped at (longest secret − 1): a full-length match would already have been
/// replaced by `redactBytes`, so only a strictly-partial head can remain. 0
/// when no suffix is a viable secret head.
///
/// Per secret, we jump straight to the candidate start positions (where the
/// secret's first byte occurs in `buf`'s tail window) via a SIMD scalar search
/// rather than re-testing every length — so the common no-overlap chunk costs
/// one vectorized scan per secret, not an O(k²) sweep. The running `best` prunes
/// further work: a candidate shorter than the best found so far can't win, and
/// because earlier positions yield longer overlaps, the first such position ends
/// the secret's scan. Bounds the per-chunk cost even when `secrets_map` carries
/// many leaf credentials.
fn pendingPrefixLen(buf: []const u8, secrets: []const Secret) usize {
    var best: usize = 0;
    for (secrets) |s| {
        const v = s.value;
        if (v.len <= 1) continue; // a 1-byte secret is whole-or-absent, never a partial head
        const window_start = buf.len - @min(v.len - 1, buf.len);
        var i = window_start;
        while (std.mem.indexOfScalarPos(u8, buf, i, v[0])) |p| {
            const k = buf.len - p; // candidate overlap if buf[p..] is a prefix of v
            if (k <= best) break; // later positions only yield smaller k — done with this secret
            if (std.mem.eql(u8, buf[p..], v[0..k])) {
                best = k;
                break; // earliest position = longest overlap for this secret
            }
            i = p + 1;
            if (i >= buf.len) break;
        }
    }
    return best;
}

// ── Tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

test "a secret split across two deltas is redacted, never streamed raw" {
    const alloc = testing.allocator;
    const secrets = [_]Secret{.{ .value = "sk-abc123", .placeholder = "[REDACTED]" }};
    var carry: std.ArrayListUnmanaged(u8) = .empty;
    defer carry.deinit(alloc);

    // Delta 1 ends mid-secret: the "sk-ab" head must be held, not emitted.
    const e1 = try push(alloc, &carry, "hello sk-ab", &secrets);
    defer alloc.free(e1);
    try testing.expectEqualStrings("hello ", e1);
    try testing.expect(std.mem.indexOf(u8, e1, "sk-ab") == null);

    // Delta 2 completes the secret across the seam → redacted in the join.
    const e2 = try push(alloc, &carry, "c123 world", &secrets);
    defer alloc.free(e2);
    try testing.expectEqualStrings("[REDACTED] world", e2);
    try testing.expect(std.mem.indexOf(u8, e2, "sk-abc123") == null);

    // The full secret never appeared on the wire across either emit.
    try testing.expect(std.mem.indexOf(u8, e1, "sk-abc123") == null);
}

test "a non-secret tail is held only until it can no longer start a secret" {
    const alloc = testing.allocator;
    const secrets = [_]Secret{.{ .value = "sk-abc123", .placeholder = "[REDACTED]" }};
    var carry: std.ArrayListUnmanaged(u8) = .empty;
    defer carry.deinit(alloc);

    // "...sk-" is a viable head → held back.
    const e1 = try push(alloc, &carry, "plain sk-", &secrets);
    defer alloc.free(e1);
    try testing.expectEqualStrings("plain ", e1);

    // Next delta diverges from the secret immediately → the held head is no
    // longer a secret prefix and flushes out verbatim.
    const e2 = try push(alloc, &carry, "NOPE done", &secrets);
    defer alloc.free(e2);
    try testing.expectEqualStrings("sk-NOPE done", e2);
}

test "no secrets is a passthrough that never grows the carry" {
    const alloc = testing.allocator;
    var carry: std.ArrayListUnmanaged(u8) = .empty;
    defer carry.deinit(alloc);
    const emit = try push(alloc, &carry, "anything at all", &[_]Secret{});
    defer alloc.free(emit);
    try testing.expectEqualStrings("anything at all", emit);
    try testing.expectEqual(@as(usize, 0), carry.items.len);
}

test "pendingPrefixLen matches a brute-force reference (optimization is behaviour-preserving)" {
    // Naive O(k^2) reference: longest k < len(some secret) with buf suffix == secret prefix.
    const ref = struct {
        fn len(buf: []const u8, secrets: []const Secret) usize {
            var max_len: usize = 0;
            for (secrets) |s| {
                if (s.value.len > max_len) max_len = s.value.len;
            }
            if (max_len == 0) return 0;
            var k = @min(max_len - 1, buf.len);
            while (k > 0) : (k -= 1) {
                for (secrets) |s| {
                    if (k >= s.value.len) continue;
                    if (std.mem.eql(u8, buf[buf.len - k ..], s.value[0..k])) return k;
                }
            }
            return 0;
        }
    }.len;

    const secrets = [_]Secret{
        .{ .value = "sk-abc123", .placeholder = "[A]" },
        .{ .value = "tok-XY", .placeholder = "[B]" },
        .{ .value = "s", .placeholder = "[C]" }, // 1-byte: never a partial head
    };
    const bufs = [_][]const u8{
        "",                  "x",                "sk",            "plain sk-abc",
        "ends with tok-X",   "sk-abc123 done",   "noise tok-",    "aaaask-ab",
        "sk-abc12",          "trailing s",       "tok-XYno",      "sk-",
    };
    for (bufs) |b| {
        try testing.expectEqual(ref(b, &secrets), pendingPrefixLen(b, &secrets));
    }
}

test "OOM leaves the carry untouched (fail-closed, no half-applied delta)" {
    const alloc = testing.allocator;
    const secrets = [_]Secret{.{ .value = "sk-abc123", .placeholder = "[REDACTED]" }};
    var carry: std.ArrayListUnmanaged(u8) = .empty;
    defer carry.deinit(alloc);

    // Seed a held tail through a real push.
    const e1 = try push(alloc, &carry, "x sk-ab", &secrets);
    defer alloc.free(e1);
    const carry_before = try alloc.dupe(u8, carry.items);
    defer alloc.free(carry_before);

    var fa = std.testing.FailingAllocator.init(alloc, .{ .fail_index = 0 });
    try testing.expectError(error.OutOfMemory, push(fa.allocator(), &carry, "c123", &secrets));
    // The carry is exactly what it was — the failed delta was not absorbed.
    try testing.expectEqualStrings(carry_before, carry.items);
}
