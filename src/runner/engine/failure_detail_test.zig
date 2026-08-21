//! Proofs for the cause line an operator reads when a run dies.
//!
//! The regression these pin: a fleet on a model no provider serves and a fleet
//! holding a rejected credential both surfaced as the bare word `ApiError`,
//! because the boundary reported `@errorName` and dropped what the provider
//! had actually said. Every test here asks the same question in a different
//! way — does the reported line let someone diagnose without a packet trace?

const std = @import("std");
const nullclaw = @import("nullclaw");
const logging = @import("log");

const failure_detail = @import("failure_detail.zig");
const runner_progress = @import("runner_progress.zig");
const types = @import("types.zig");

const ALLOC = std.testing.allocator;

/// The shape Fireworks returns for the mistyped-model fault this exists for.
const PROVIDER_NAME = "compatible";
const MODEL_NOT_FOUND = "status=404 message=Model not found, inaccessible, and/or not deployed";

fn setProviderDetail(detail: []const u8) void {
    nullclaw.providers.setLastApiErrorDetail(PROVIDER_NAME, detail);
}

test "with no provider fault the line is the error name alone" {
    // The pre-model paths (bad config, no instructions) never dial, so there
    // is nothing to add and the name is the whole truth.
    failure_detail.clear();
    failure_detail.capture(ALLOC, &.{});
    try std.testing.expectEqualStrings("FleetInitFailed", failure_detail.compose(error.FleetInitFailed));
}

test "a provider fault is reported with the provider's own words" {
    // The fix itself: `ApiError` alone sent an engineer to strace a host to
    // learn what the API had already said in plain text.
    failure_detail.clear();
    setProviderDetail(MODEL_NOT_FOUND);
    failure_detail.capture(ALLOC, &.{});

    const line = failure_detail.compose(error.ApiError);
    try std.testing.expect(std.mem.startsWith(u8, line, "ApiError: "));
    try std.testing.expect(std.mem.indexOf(u8, line, "404") != null);
    try std.testing.expect(std.mem.indexOf(u8, line, "Model not found") != null);
    // The provider name rides along: with several configured, "which one
    // rejected us" is the next question an operator asks.
    try std.testing.expect(std.mem.indexOf(u8, line, PROVIDER_NAME) != null);
}

test "the error name always leads, so the registry key survives" {
    // Logs and the error registry are keyed on the name. A line that dropped
    // it for provider prose would break the grep that finds the event.
    failure_detail.clear();
    setProviderDetail(MODEL_NOT_FOUND);
    failure_detail.capture(ALLOC, &.{});
    try std.testing.expect(std.mem.startsWith(u8, failure_detail.compose(error.FleetRunFailed), "FleetRunFailed:"));
}

test "a cause line carrying a resolved secret is withheld whole, never partially scrubbed" {
    // RULE VLT. The value is a tenant secret that looks like ordinary text,
    // which is exactly what NullClaw's key-SHAPED pattern scrub cannot catch —
    // so this boundary is the one that has to.
    const secret_value = "hunter2-not-a-key-shape";
    failure_detail.clear();
    var buf: [128]u8 = undefined;
    const echoed = try std.fmt.bufPrint(&buf, "status=400 message=bad value {s}", .{secret_value});
    setProviderDetail(echoed);

    const secrets = [_]runner_progress.Secret{
        .{ .value = secret_value, .placeholder = "${secrets.llm.api_key}" },
    };
    failure_detail.capture(ALLOC, &secrets);

    const line = failure_detail.compose(error.ApiError);
    try std.testing.expect(std.mem.indexOf(u8, line, secret_value) == null);
    try std.testing.expect(std.mem.indexOf(u8, line, failure_detail.DETAIL_WITHHELD) != null);
}

test "an empty secret slot matches nothing, so an unset api_key never withholds every line" {
    // The `api_key` slot always exists even when unset; treating "" as a
    // substring would suppress the detail on every run that has no key.
    failure_detail.clear();
    setProviderDetail(MODEL_NOT_FOUND);
    const secrets = [_]runner_progress.Secret{
        .{ .value = "", .placeholder = "${secrets.llm.api_key}" },
    };
    failure_detail.capture(ALLOC, &secrets);
    try std.testing.expect(std.mem.indexOf(u8, failure_detail.compose(error.ApiError), "Model not found") != null);
}

test "an over-long provider line is truncated with a mark, not silently cut" {
    failure_detail.clear();
    const long = try ALLOC.alloc(u8, failure_detail.MAX_DETAIL_BYTES * 2);
    defer ALLOC.free(long);
    @memset(long, 'x');
    setProviderDetail(long);
    failure_detail.capture(ALLOC, &.{});

    const line = failure_detail.compose(error.ApiError);
    try std.testing.expect(line.len <= failure_detail.MAX_DETAIL_BYTES + 64);
    try std.testing.expect(std.mem.endsWith(u8, line, "…"));
}

test "truncation lands on a character boundary, so the stored line stays valid UTF-8" {
    // A split multi-byte sequence renders as a replacement glyph, which an
    // operator reads as corruption rather than as a cut.
    failure_detail.clear();
    const multi = "é";
    const count = failure_detail.MAX_DETAIL_BYTES; // 2 bytes each ⇒ overruns
    const long = try ALLOC.alloc(u8, count * multi.len);
    defer ALLOC.free(long);
    var i: usize = 0;
    while (i < count) : (i += 1) @memcpy(long[i * multi.len ..][0..multi.len], multi);
    setProviderDetail(long);
    failure_detail.capture(ALLOC, &.{});

    try std.testing.expect(std.unicode.utf8ValidateSlice(failure_detail.compose(error.ApiError)));
}

test "clear drops the captured line so a later failure is graded on its own evidence" {
    // A stale snapshot attached to an unrelated later error is worse than no
    // detail: it sends the reader after a fault that already passed.
    failure_detail.clear();
    setProviderDetail(MODEL_NOT_FOUND);
    failure_detail.capture(ALLOC, &.{});
    try std.testing.expect(std.mem.indexOf(u8, failure_detail.compose(error.ApiError), "404") != null);

    failure_detail.clear();
    failure_detail.capture(ALLOC, &.{});
    try std.testing.expectEqualStrings("ApiError", failure_detail.compose(error.ApiError));
}

test "every mapped class survives the move out of runner.zig" {
    try std.testing.expectEqual(types.FailureClass.startup_posture, failure_detail.mapError(error.InvalidConfig));
    try std.testing.expectEqual(types.FailureClass.startup_posture, failure_detail.mapError(error.FleetInitFailed));
    try std.testing.expectEqual(types.FailureClass.timeout_kill, failure_detail.mapError(error.Timeout));
    try std.testing.expectEqual(types.FailureClass.oom_kill, failure_detail.mapError(error.OutOfMemory));
    try std.testing.expectEqual(types.FailureClass.runner_crash, failure_detail.mapError(error.FleetRunFailed));
    try std.testing.expectEqual(types.FailureClass.runner_crash, failure_detail.mapError(error.Unexpected));
}

test "a provider line with spaces and '=' cannot break the logfmt record" {
    // The cause line is the first provider-authored text to reach a log field,
    // and it is full of exactly the characters logfmt uses as structure:
    // `message=Model not found` would otherwise read as its own key. Unquoted,
    // one bad model name silently corrupts every downstream log query.
    failure_detail.clear();
    setProviderDetail(MODEL_NOT_FOUND);
    failure_detail.capture(ALLOC, &.{});

    var bs = logging.sinks.BufferedSink.init(ALLOC);
    defer bs.deinit();
    logging.sinks.clearSinksForTest();
    defer logging.sinks.clearSinksForTest();
    logging.sinks.registerSink(bs.sink());

    const log = logging.scoped(.runner);
    log.err("runner_execute_failed", .{ .err = failure_detail.compose(error.ApiError) });

    const captured = try bs.snapshot();
    defer ALLOC.free(captured);
    // Quoted as ONE value, enclosing the provider's own `message=` rather than
    // letting it read as a second key. Asserted as the WHOLE expected value:
    // checking only for the opening `err="` would still pass if the encoder
    // closed the quote early and spilled the rest into the record.
    var want_buf: [256]u8 = undefined;
    const want = try std.fmt.bufPrint(&want_buf, "err=\"ApiError: {s}: {s}\"", .{ PROVIDER_NAME, MODEL_NOT_FOUND });
    try std.testing.expect(std.mem.indexOf(u8, captured, want) != null);
}

test "test_config_load_failure_names_error: the record carries the cause, not just the code" {
    // The regression: `Config.load(alloc) catch { log.err(...) }` discarded the
    // error, so a dev fleet where every lease died at init logged only
    // UZ-EXEC-012 with no cause — and the real fault (no HOME in the daemon's
    // environment) could only be found by reproducing it on the host.
    var bs = logging.sinks.BufferedSink.init(ALLOC);
    defer bs.deinit();

    logging.sinks.clearSinksForTest();
    defer logging.sinks.clearSinksForTest();
    logging.sinks.registerSink(bs.sink());

    failure_detail.logConfigLoadFailure(error.NoHomeDir);

    const captured = try bs.snapshot();
    defer ALLOC.free(captured);
    try std.testing.expect(std.mem.indexOf(u8, captured, "config_load_failed") != null);
    try std.testing.expect(std.mem.indexOf(u8, captured, "UZ-EXEC-012") != null);
    // The assertion that would have failed before the fix.
    try std.testing.expect(std.mem.indexOf(u8, captured, "NoHomeDir") != null);
}
