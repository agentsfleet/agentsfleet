//! failure_detail.zig — how an engine error becomes a REPORTED failure: the
//! class the event surface shows, and the cause line underneath it.
//!
//! Split from `runner.zig` on the file-length bound (RULE FLL), along the
//! seam that matters: that file runs the fleet, this one decides what an
//! operator is told when the run dies.
//!
//! The cause line exists because `@errorName` alone is unreadable. A fleet
//! pinned to a model no provider serves and a fleet holding a rejected
//! credential both surface as `ApiError` — NullClaw's `error_classify`
//! collapses every non-rate-limit, non-context, non-vision fault into one
//! bucket — so the event named neither the model nor the status. The provider's
//! own words ("Model not found, inaccessible, and/or not deployed") were
//! already captured inside the child and thrown away at this boundary; a
//! mistyped model cost days of tracing to learn what the API had said plainly.
//!
//! Allocation-free by construction on the reporting path. The error being
//! reported may BE an allocation failure, so the composed line lands in
//! process-static buffers rather than the heap — the same shape NullClaw uses
//! for the snapshot this reads. Safe because a `__execute` child runs exactly
//! one lease and then exits; the returned slice stays valid until process exit.

const std = @import("std");
const nullclaw = @import("nullclaw");
const logging = @import("log");

const types = @import("types.zig");
const client_errors = @import("client_errors.zig");
const runner_progress = @import("runner_progress.zig");

const log = logging.scoped(.runner);

/// Cap on the provider's cause line. NullClaw bounds its own snapshot at 2048;
/// this is tighter because the value is persisted per event and read in a
/// dashboard cell, where the first line carries the diagnosis and the rest is
/// noise. Truncation is marked, never silent.
pub const MAX_DETAIL_BYTES: usize = 512;

/// Longest `@errorName` this composes with, plus the `": "` join. Sized from
/// the error set rather than guessed: the longest member is
/// `ProviderDoesNotSupportVision` (28).
const MAX_NAME_BYTES: usize = 64;

/// Marker replacing a cause line that contained a tenant secret. Fail CLOSED:
/// the detail is DROPPED whole rather than partially redacted, because a
/// redactor that runs on the error path can itself fail, and a half-scrubbed
/// line is a leak (RULE VLT). The operator still learns a detail existed and
/// why it is missing, which a silent empty string would not tell them.
pub const DETAIL_WITHHELD = "<provider detail withheld: contained a resolved secret>";

/// Marker for a snapshot this could not take — the only failure `capture` can
/// hit, and it means the fault was memory. Recorded rather than swallowed: an
/// empty cause line is indistinguishable from "the provider said nothing", and
/// the difference is exactly what an operator needs when the engine is dying
/// of exhaustion. `capture` runs from an `errdefer`, which cannot propagate,
/// so leaving a mark IS how it reports.
pub const DETAIL_UNAVAILABLE = "<provider detail unavailable: snapshot allocation failed>";

/// Appended when the provider's line exceeds `MAX_DETAIL_BYTES`.
const TRUNCATION_MARK = "…";

var detail_buf: [MAX_DETAIL_BYTES]u8 = undefined;
var detail_len: usize = 0;
var composed_buf: [MAX_DETAIL_BYTES + MAX_NAME_BYTES + 2]u8 = undefined;

/// Drop any captured cause line. For tests, and for a caller that wants the
/// next failure graded on its own evidence rather than a stale snapshot.
pub fn clear() void {
    detail_len = 0;
    nullclaw.providers.clearLastApiErrorDetail();
}

/// Capture whatever the engine last recorded about a provider fault, scrubbed
/// against the run's own secret set.
///
/// It runs while an error is already unwinding — from an `errdefer`, which
/// cannot propagate — so it returns `void`. That is NOT licence to swallow:
/// every outcome it can reach leaves a readable mark. A snapshot that fails
/// records `DETAIL_UNAVAILABLE`, a line carrying a secret records
/// `DETAIL_WITHHELD`, and only a genuinely absent snapshot (no provider was
/// ever dialed) leaves the line empty, which `compose` reports as the bare
/// error name.
///
/// `secrets` is the substitution set, so the check covers exactly the values
/// the engine could have resolved into an outbound request. NullClaw scrubs
/// key-SHAPED patterns before it stores the line, which is a different set: it
/// cannot know a tenant secret whose value looks like ordinary text.
pub fn capture(alloc: std.mem.Allocator, secrets: []const runner_progress.Secret) void {
    detail_len = 0;
    const snapshot = nullclaw.providers.snapshotLastApiErrorDetail(alloc) catch {
        detail_len = copyBounded(&detail_buf, DETAIL_UNAVAILABLE);
        return;
    };
    // No snapshot means no provider was dialed — the error came from before
    // the model call, and the error name is the whole truth. Distinct from a
    // failed snapshot above, which had something to say and could not say it.
    const owned = snapshot orelse return;
    defer alloc.free(owned);
    if (owned.len == 0) return;

    if (containsSecret(owned, secrets)) {
        detail_len = copyBounded(&detail_buf, DETAIL_WITHHELD);
        return;
    }
    detail_len = copyBounded(&detail_buf, owned);
}

/// Does this line carry any resolved secret verbatim? Empty values never match
/// — an unset `api_key` slot is a hole in the map, not a substring every line
/// trivially contains.
fn containsSecret(line: []const u8, secrets: []const runner_progress.Secret) bool {
    for (secrets) |s| {
        if (s.value.len == 0) continue;
        if (std.mem.indexOf(u8, line, s.value) != null) return true;
    }
    return false;
}

/// Copy `src` into `dst`, marking a truncation rather than hiding it. Returns
/// the byte count written.
fn copyBounded(dst: []u8, src: []const u8) usize {
    if (src.len <= dst.len) {
        @memcpy(dst[0..src.len], src);
        return src.len;
    }
    // Truncate on a UTF-8 boundary so the stored line is never invalid — a
    // dashboard cell renders a split sequence as a replacement glyph and the
    // operator reads it as corruption rather than as a cut.
    var cut = dst.len - TRUNCATION_MARK.len;
    while (cut > 0 and (src[cut] & 0xC0) == 0x80) cut -= 1;
    @memcpy(dst[0..cut], src[0..cut]);
    @memcpy(dst[cut..][0..TRUNCATION_MARK.len], TRUNCATION_MARK);
    return cut + TRUNCATION_MARK.len;
}

/// The cause line for `err`: its name, plus the provider's own words when a
/// capture took them.
///
/// The name ALWAYS leads. It is the one fact every failure has, it is what the
/// error registry and the logs are keyed on, and a line that dropped it to
/// make room for provider prose would break the grep that finds the event.
pub fn compose(err: anyerror) []const u8 {
    const name = @errorName(err);
    if (detail_len == 0) return name;
    if (name.len > MAX_NAME_BYTES) return name;

    @memcpy(composed_buf[0..name.len], name);
    composed_buf[name.len] = ':';
    composed_buf[name.len + 1] = ' ';
    const at = name.len + 2;
    @memcpy(composed_buf[at..][0..detail_len], detail_buf[0..detail_len]);
    return composed_buf[0 .. at + detail_len];
}

/// Record a config-load failure WITH its cause. Lives here because it answers
/// the same question the cause line does — what an operator is told when a run
/// dies. This runs inside the sandboxed child, where the usual fault is an
/// environment the cage did not carry (`NoHomeDir` when the daemon itself has
/// no HOME). Dropping the error name leaves the journal showing a code and
/// nothing else, which is the difference between reading the fault and
/// reproducing it on the host to find it.
pub fn logConfigLoadFailure(err: anyerror) void {
    log.err("config_load_failed", .{ .error_code = client_errors.ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err) });
}

/// Map an engine error to the class the event surface groups on.
pub fn mapError(err: anyerror) types.FailureClass {
    return switch (err) {
        error.InvalidConfig => .startup_posture,
        error.FleetInitFailed => .startup_posture,
        error.Timeout => .timeout_kill,
        error.OutOfMemory => .oom_kill,
        error.FleetRunFailed => .runner_crash,
        else => .runner_crash,
    };
}

test {
    _ = @import("failure_detail_test.zig");
}
