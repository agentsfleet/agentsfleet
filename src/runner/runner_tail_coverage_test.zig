//! The runner lane's remaining reachable arms, gathered where each is cheapest
//! to drive rather than spread across five near-empty sibling files:
//!
//!   - `runner_helpers.collectSecrets` under EXHAUSTIVE allocation failure —
//!     the only proof that its hand-rolled errdefer ladder frees on every
//!     partial build (it duplicates two owned fields per secret and unwinds by
//!     hand, so a missed free is invisible to the happy path);
//!   - `buildToolsFromSpec`'s two fall-back-to-default arms;
//!   - `policy_apply`'s invalid / cleared heartbeat arms, which decide whether a
//!     runner leases at all;
//!   - `StorageHome.claimAndSweep`'s two refusals — a home that cannot be opened
//!     and one whose canonical path is too shallow to sweep (that guard is what
//!     stands between a stray config value and `deleteTree` on a real directory);
//!   - `cmd/help`'s two render-to-stream entry points, with stdout muted so the
//!     suite never writes to a real terminal.

const std = @import("std");
const testing = std.testing;
const common = @import("common");

const runner_helpers = @import("engine/runner_helpers.zig");
const policy_apply = @import("daemon/policy_apply.zig");
const AppliedPolicy = @import("daemon/AppliedPolicy.zig");
const StorageHome = @import("daemon/StorageHome.zig");
const help = @import("cmd/help.zig");
const muted = @import("cmd/plane_stub_test.zig");

const ALLOC = testing.allocator;

/// One provider key plus a two-field tool credential — enough shape that the
/// collector walks both its loops and every allocation site inside them.
const FLEET_CONFIG_JSON =
    \\{"api_key":"sk-provider-key"}
;
const SECRETS_MAP_JSON =
    \\{"github":{"token":"ghs_tok","user":"octo"}}
;
/// One-worker assignment in the wire shape `AppliedPolicy.apply` decodes. The
/// tier is incidental to the arms below, but it cannot be `dev_none`:
/// `policy_apply.devNoneForbidden` refuses that tier in every build mode except
/// Debug, so the holder would clear instead of applying under the Valgrind lane
/// (`scripts/run-zig-memleak-lane.sh` pins `-Doptimize=ReleaseSafe`).
const POLICY_JSON =
    \\{"sandbox_tier":"landlock_full","network_policy":"deny_all_egress","registry_allowlist":[],"worker_count":1}
;
/// A path no daemon could open — the parent is a char device, so the open
/// itself fails rather than the canonicalize below it.
const UNOPENABLE_HOME = "/dev/null/storage-home";
/// The filesystem root — depth 0, so the guard refuses it before the claim.
/// Deliberately NOT a temp directory: `/tmp` canonicalizes to `/private/tmp`
/// (depth 2) and is ADOPTED, which would have this test claim a real shared
/// directory and run an orphan sweep across it.
const SHALLOW_HOME = "/";
/// An unknown verb the help renderer echoes back before the usage block.
const UNKNOWN_COMMAND = "flibbertigibbet";

fn parse(json_text: []const u8) !std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, ALLOC, json_text, .{});
}

/// Collect and free under one allocator — the shape `checkAllAllocationFailures`
/// drives once per allocation site, asserting nothing leaks on any early return.
fn collectAndFree(alloc: std.mem.Allocator, cfg: std.json.Value, secrets: std.json.Value) !void {
    const out = try runner_helpers.collectSecrets(alloc, cfg, secrets);
    runner_helpers.freeSecrets(alloc, out);
}

test "collectSecrets frees every partially built secret on any allocation failure" {
    const cfg = try parse(FLEET_CONFIG_JSON);
    defer cfg.deinit();
    const secrets = try parse(SECRETS_MAP_JSON);
    defer secrets.deinit();
    // Exhaustive: fails each allocation site in turn and asserts the unwind
    // released everything already built. The collector owns two duped fields per
    // secret and unwinds by hand, so this is the only check that every errdefer
    // in that ladder is correct.
    try testing.checkAllAllocationFailures(ALLOC, collectAndFree, .{ cfg.value, secrets.value });
}

test "collectSecrets yields the provider key and every tool credential field, fully owned" {
    const cfg = try parse(FLEET_CONFIG_JSON);
    defer cfg.deinit();
    const secrets = try parse(SECRETS_MAP_JSON);
    defer secrets.deinit();
    const out = try runner_helpers.collectSecrets(ALLOC, cfg.value, secrets.value);
    defer runner_helpers.freeSecrets(ALLOC, out);
    // api_key slot + one placeholder per credential field.
    try testing.expectEqual(@as(usize, 3), out.len);
    try testing.expectEqualStrings("sk-provider-key", out[0].value);
    try testing.expectEqualStrings("${secrets.llm.api_key}", out[0].placeholder);
    // Every field is owned by `alloc`, never borrowed from the caller's parse —
    // a borrow here would dangle the moment the lease JSON is freed.
    for (out) |s| {
        try testing.expect(s.value.ptr != FLEET_CONFIG_JSON.ptr);
        try testing.expect(s.placeholder.len > 0);
    }
}

test "collectSecrets still yields the api_key slot when there is no config at all" {
    const out = try runner_helpers.collectSecrets(ALLOC, null, null);
    defer runner_helpers.freeSecrets(ALLOC, out);
    // The slot exists even empty: an empty value short-circuits redaction, and a
    // missing slot would change the redaction set's shape rather than its content.
    try testing.expectEqual(@as(usize, 1), out.len);
    try testing.expectEqual(@as(usize, 0), out[0].value.len);
}

test "an invalid or absent assignment drops the held policy and stops leasing" {
    var applied = AppliedPolicy.init(ALLOC);
    defer applied.deinit();
    var gates = policy_apply.Gates{};

    const good = try parse(POLICY_JSON);
    defer good.deinit();
    // A JSON scalar where an object belongs: unreadable, so nothing is held.
    const bad = try parse("\"not-a-policy\"");
    defer bad.deinit();

    policy_apply.applyHeartbeatPolicy(ALLOC, &applied, &gates, good.value);
    try testing.expectEqual(AppliedPolicy.ApplyOutcome.applied, gates.last_outcome);
    try testing.expectEqual(@as(?u32, 1), applied.currentWorkerCount());

    // An unreadable assignment DROPS the working one rather than keeping it:
    // fail-closed, so a control plane that starts emitting garbage stops the
    // leasing it was authorising a beat ago.
    policy_apply.applyHeartbeatPolicy(ALLOC, &applied, &gates, bad.value);
    try testing.expectEqual(AppliedPolicy.ApplyOutcome.invalid, gates.last_outcome);
    try testing.expectEqual(@as(?u32, null), applied.currentWorkerCount());

    // A second identical bad beat is `unchanged`, not a fresh `invalid` — the
    // holder already reflects it, so a plane stuck on garbage cannot reflood.
    policy_apply.applyHeartbeatPolicy(ALLOC, &applied, &gates, bad.value);
    try testing.expectEqual(AppliedPolicy.ApplyOutcome.unchanged, gates.last_outcome);

    // Re-assign, then withdraw entirely: absence clears the holder explicitly.
    policy_apply.applyHeartbeatPolicy(ALLOC, &applied, &gates, good.value);
    try testing.expectEqual(AppliedPolicy.ApplyOutcome.applied, gates.last_outcome);
    policy_apply.applyHeartbeatPolicy(ALLOC, &applied, &gates, null);
    try testing.expectEqual(AppliedPolicy.ApplyOutcome.cleared, gates.last_outcome);
    try testing.expectEqual(@as(?u32, null), applied.currentWorkerCount());
}

test "a storage home that cannot be opened degrades to unavailable, never fails the daemon" {
    const startup = StorageHome.claimAndSweep(common.globalIo(), UNOPENABLE_HOME);
    // No claim, no reaping, and the daemon carries on — an unclaimable home is
    // logged, not fatal.
    try testing.expectEqual(StorageHome.Outcome.unavailable, startup.outcome);
    try testing.expect(startup.home == null);
}

test "a storage home whose canonical path is too shallow is refused before any sweep" {
    var startup = StorageHome.claimAndSweep(common.globalIo(), SHALLOW_HOME);
    defer if (startup.home) |*h| h.close(common.globalIo());
    // The guard between a stray config value and `deleteTree` over a real
    // directory's children. It refuses on DEPTH alone, so it only catches the
    // shallowest mistakes — a two-component path clears it (see SHALLOW_HOME).
    try testing.expectEqual(StorageHome.Outcome.refused_shallow, startup.outcome);
    try testing.expect(startup.home == null);
}

test "help renders every registered command to stdout and exits 0" {
    var sink = muted.MutedStdout.mute() catch return error.SkipZigTest;
    defer sink.restore();
    // Exit code is the whole caller-visible outcome; the body is pinned
    // byte-for-byte by the renderer's own test against the captured fixture.
    try testing.expectEqual(@as(u8, 0), help.run(ALLOC));
}

test "an unknown command renders help and exits 2" {
    var sink = muted.MutedStdout.mute() catch return error.SkipZigTest;
    defer sink.restore();
    // 2, not 0: a mistyped verb is a usage error a script must be able to detect.
    try testing.expectEqual(@as(u8, 2), help.runUnknown(ALLOC, UNKNOWN_COMMAND));
}
