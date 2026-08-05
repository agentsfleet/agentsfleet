//! Dimensions 3.3 and 3.4 — the repairer's write lives behind the gate, proven
//! through the real lease path against the SHIPPED bundle's own configuration.
//!
//! The generic gate lifecycle is already covered and is not repeated here:
//! parking, denial, and deadline expiry each have a test against a hand-written
//! `lifecycle-gated` config (`event_lifecycle_integration_test.zig`,
//! `event_lifecycle_reclaim_integration_test.zig`). Re-proving the machinery
//! would say nothing about this milestone.
//!
//! What those do NOT cover is the claim M157 actually makes: that the bundle we
//! SHIP, as written, cannot run without a human. So these tests read
//! `library/incident-repairer/TRIGGER.md` off disk, convert its frontmatter the
//! same way the install path does (`yamlFrontmatterToJson`), and seed that as
//! the fleet's `config_json`. Edit the bundle to drop `gates`, or to a rule that
//! matches nothing at this call site, and these fail — which is the entire
//! point: `fleet_runtime/approval_gate.zig:96` falls through to `.auto_approve`,
//! so a missing or non-matching rule is not "ask about nothing", it is an
//! autonomous agent holding a write token.
//!
//! **"No Pull Request" is structural, not observed.** No lease is issued, so no
//! child is forked, so no tool call happens and no vendor is reached. There is
//! no fake GitHub here because there is nothing for it to watch — the run never
//! starts. The vendor-side half of the boundary is Dimension 3.1's, where the
//! shipped read binding is driven through the real mint.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;
const common = @import("common");

const life = @import("event_lifecycle_integration_test.zig");
const event_rows = @import("event_rows.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const approval_gate_async = @import("../fleet_runtime/approval_gate_async.zig");
const gate_constants = @import("../fleet_runtime/approval_gate_constants.zig");
const config = @import("../fleet_runtime/config.zig");
const vault = @import("../state/vault.zig");

const ALLOC = std.testing.allocator;

/// Owned by this file alone — `life.Env.deinit` purges a fixed fleet list that
/// does not include this one, so the Redis footprint is dropped here by hand.
const FLEET_REPAIRER = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7d01";

const LIBRARY_BASE = "library";
const REPAIRER_SLUG = "incident-repairer";
const TRIGGER_MD = "TRIGGER.md";
const BYTES_PER_KIB = 1024;
const MAX_BUNDLE_BYTES = 64 * BYTES_PER_KIB;

/// The repairer declares the `github` credential. Secrets resolve BEFORE the
/// gate is consulted (`service.zig:111-118` → `runBilling`), so an unseeded
/// credential would refuse the lease with `secret_missing` and the event would
/// never reach the gate at all — the test would pass for the wrong reason.
const CREDENTIAL_GITHUB = "github";
const HANDLE_GITHUB = "{\"integration\":\"github\",\"installation_id\":\"42\"}";

const DECISION_TTL_S: i64 = 60;

/// The shipped bundle's frontmatter, as the fleet row would carry it. This is
/// the install path's OWN conversion (`parseTriggerMarkdownWithJson`), not a
/// re-implementation — so a bundle that cannot become a config here could not
/// have become a fleet in production either. Caller owns.
fn shippedRepairerConfig(alloc: std.mem.Allocator) ![]u8 {
    const path = try std.fs.path.join(alloc, &.{ LIBRARY_BASE, REPAIRER_SLUG, TRIGGER_MD });
    defer alloc.free(path);
    const md = try std.Io.Dir.cwd().readFileAlloc(common.globalIo(), path, alloc, .limited(MAX_BUNDLE_BYTES));
    defer alloc.free(md);
    var parsed = try config.parseTriggerMarkdownWithJson(alloc, md);
    defer parsed.deinit(alloc);
    return alloc.dupe(u8, parsed.config_json);
}

/// Seed the repairer as a real fleet carrying its own shipped configuration.
fn seedShippedRepairer(conn: *pg.Conn) !void {
    const config_json = try shippedRepairerConfig(ALLOC);
    defer ALLOC.free(config_json);
    try life.seedFleetWithConfig(conn, FLEET_REPAIRER, REPAIRER_SLUG, config_json);
    try vault.storeJsonPlaintext(ALLOC, conn, life.WORKSPACE_ID, CREDENTIAL_GITHUB, HANDLE_GITHUB);
}

fn forgetRepairer(h: anytype) void {
    redis_fleet.purgeFleetRedisState(&h.queue, FLEET_REPAIRER) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

/// The action id the daemon parked this event under.
fn gateRefFor(h: anytype, event_id: []const u8) !approval_gate_async.EventGateRef {
    const maybe_ref = try approval_gate_async.lookupEventGateRef(&h.queue, FLEET_REPAIRER, event_id);
    return maybe_ref orelse error.GateRefMissing;
}

/// Write the decision the Slack approval webhook would write.
fn resolveGate(h: anytype, ref: *const approval_gate_async.EventGateRef, decision: []const u8) !void {
    var key_buf: [256]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "{s}{s}", .{ gate_constants.GATE_RESPONSE_KEY_PREFIX, ref.actionId() });
    try h.queue.setEx(key, decision, DECISION_TTL_S);
}

// ── Dimension 3.3 ───────────────────────────────────────────────────────────

test "test_unapproved_event_opens_no_pr" {
    // A wake reaches the repairer and no human has answered. The event parks:
    // no lease is issued, so no child is forked, so nothing can call GitHub.
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer forgetRepairer(h);

    try seedShippedRepairer(conn);

    const event_id = try life.publishEvent(h, FLEET_REPAIRER);
    defer h.queue.alloc.free(event_id);

    // No lease. This is the Dimension: the run that would open the Pull Request
    // is never issued, and the reason is the bundle's own gate rule.
    try std.testing.expect(!try life.pollLease(h));
    // Parked, NOT terminal — the question is outstanding, so the entry stays in
    // the Pending Entries List for the next poll to re-evaluate.
    try life.expectRow(conn, FLEET_REPAIRER, event_id, event_rows.STATUS_RECEIVED, "");
    try std.testing.expectEqual(@as(i64, 1), try life.pendingCount(h, FLEET_REPAIRER));

    // And the daemon actually raised a question rather than merely declining —
    // a parked event with no recorded gate ref would be a fleet stuck forever.
    const ref = try gateRefFor(h, event_id);
    try std.testing.expect(ref.actionId().len > 0);

    // Polling again changes nothing: still no lease, still no Pull Request. A
    // fleet does not wear its gate down by being asked twice.
    try std.testing.expect(!try life.pollLease(h));
    try life.expectRow(conn, FLEET_REPAIRER, event_id, event_rows.STATUS_RECEIVED, "");
}

// ── Dimension 3.4 ───────────────────────────────────────────────────────────

test "test_denied_or_timed_out_never_runs" {
    // Both terminal resolutions, on the shipped bundle. Neither issues a lease,
    // so neither can produce a Pull Request — and both are TERMINAL, so the
    // event is acknowledged rather than left to be re-asked forever.
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer forgetRepairer(h);

    try seedShippedRepairer(conn);

    // ── Denial ──────────────────────────────────────────────────────────────
    const denied_event = try life.publishEvent(h, FLEET_REPAIRER);
    defer h.queue.alloc.free(denied_event);

    try std.testing.expect(!try life.pollLease(h));
    const denied_ref = try gateRefFor(h, denied_event);
    try resolveGate(h, &denied_ref, gate_constants.GATE_DECISION_DENY);

    // The Pending Entries List re-delivers, the recorded gate resolves denied.
    try std.testing.expect(!try life.pollLease(h));
    try life.expectRow(conn, FLEET_REPAIRER, denied_event, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_APPROVAL_DENIED);
    try std.testing.expectEqual(@as(i64, 0), try life.pendingCount(h, FLEET_REPAIRER));

    // ── Deadline expiry ─────────────────────────────────────────────────────
    const expired_event = try life.publishEvent(h, FLEET_REPAIRER);
    defer h.queue.alloc.free(expired_event);

    try std.testing.expect(!try life.pollLease(h));
    const expired_ref = try gateRefFor(h, expired_event);

    // Move the deadline into the past rather than sleeping out the bundle's own
    // timeout. `evaluateRef` compares the recorded deadline against the clock,
    // so re-recording the ref is exactly the state a lapsed approval reaches —
    // and it is deterministic, where waiting is not.
    try approval_gate_async.recordEventGateRef(
        &h.queue,
        FLEET_REPAIRER,
        expired_event,
        expired_ref.actionId(),
        clock.nowMillis() - 1,
    );

    try std.testing.expect(!try life.pollLease(h));
    try life.expectRow(conn, FLEET_REPAIRER, expired_event, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_APPROVAL_EXPIRED);
    try std.testing.expectEqual(@as(i64, 0), try life.pendingCount(h, FLEET_REPAIRER));
}

// ── The negative control ────────────────────────────────────────────────────

test "the shipped repairer's gate rule matches the wake it actually receives" {
    // Dimension 3.2 asserts the rule parses and fires against a hand-built pair.
    // This asserts the consequence at the only site that evaluates it: if the
    // rule did not match, `route` would return `.pass` and this poll WOULD issue
    // a lease. A tool-shaped rule (`{"tool":"git","action":"push"}`) matches
    // nothing here, so that regression fails as a LEASE rather than as a silence.
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer forgetRepairer(h);

    try seedShippedRepairer(conn);
    const event_id = try life.publishEvent(h, FLEET_REPAIRER);
    defer h.queue.alloc.free(event_id);

    try std.testing.expect(!try life.pollLease(h));
    // A recorded ref exists ⇒ `request_new` ran ⇒ the rule matched. Its absence
    // would mean the event passed the gate untouched.
    _ = try gateRefFor(h, event_id);
}
