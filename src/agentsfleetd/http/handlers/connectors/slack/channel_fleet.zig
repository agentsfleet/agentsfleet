//! Resolve `(slack, team_id, channel_id)` → resident fleet_id via
//! `core.connector_channels`; on a binding miss, materialize a durable
//! per-channel resident fleet.
//!
//! Materialization reuses the shared fleet-insert path (`fleet_row.insertFleetOnConn`,
//! Invariant 7 — this file never runs `INSERT INTO core.fleets` itself) under
//! install-delegated authority (no principal in the inbound flow), seeded with
//! the embedded default channel-bot skill.md as `source_markdown` and a
//! code-built reactive config (one `api` trigger, `tools: []`, a code-set
//! budget) that is **asserted** post-parse (Invariant 2 — a prompt can be
//! injection-overridden, capability cannot).
//!
//! Concurrency (Invariant 6): the per-workspace fleet-name unique constraint
//! (`slack-channel-<channel>`) is the serialization point — a concurrent
//! same-channel first-mention collides there, and the loser converges on the
//! winner's fleet via `resolveExistingByName` instead of wedging. The
//! `connector_channels` binding is inserted `ON CONFLICT DO NOTHING`, so at most
//! one binding ever exists per channel.

const std = @import("std");
const sql = @import("sql.zig");
const pg = @import("pg");
const constants = @import("common");
const clock = constants.clock;
const logging = @import("log");

const PgQuery = @import("../../../../db/pg_query.zig").PgQuery;
const ec = @import("../../../../errors/error_registry.zig");
const fleet_config = @import("../../../../fleet_runtime/config.zig");
const id_format = @import("../../../../types/id_format.zig");
const queue_redis = @import("../../../../queue/redis_client.zig");
const fleet_row = @import("../../fleets/fleet_row.zig");
const create_stream = @import("../../fleets/create_stream.zig");
const spec = @import("spec.zig");

const log = logging.scoped(.connector_slack);

const KIND_RESIDENT = constants.CONNECTOR_CHANNEL_KIND_RESIDENT;
const CHANNEL_REF_PLACEHOLDER = "{channel_ref}";
/// Code-set daily spend ceiling for a resident fleet (a reactive answer bot is
/// modest by design; the budget is built here, never parsed from skill.md).
const RESIDENT_DAILY_DOLLARS = "1.0";
const DEFAULT_SKILL = @embedFile("channel_bot_skill.md");

/// Resolve the resident fleet for `(team_id, channel_id)`, materializing it on
/// a binding miss. Returns an owned `channel_fleet_id` (caller frees).
pub fn resolveOrCreate(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    queue: *queue_redis.Client,
    workspace_id: []const u8,
    team_id: []const u8,
    channel_id: []const u8,
) ![]const u8 {
    if (try selectBinding(alloc, conn, team_id, channel_id)) |fid| return fid;
    return materialize(alloc, conn, queue, workspace_id, team_id, channel_id);
}

fn materialize(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    queue: *queue_redis.Client,
    workspace_id: []const u8,
    team_id: []const u8,
    channel_id: []const u8,
) ![]const u8 {
    // Fleet name = `slack-channel-<team>-<channel>` (both lowercased): the
    // binding key is (team_id, channel_id), so the convergence key — the
    // per-workspace-unique fleet name — must include the team. Two Slack teams
    // mapped to one workspace could otherwise collide on a shared channel id and
    // bleed one channel's memory into the other. SKILL.md `name:` must match
    // `^[a-z0-9-]+$`, and Slack team/channel ids are uppercase alnum.
    const ref = try channelRef(alloc, team_id, channel_id);
    defer alloc.free(ref);
    const source_markdown = try renderSkill(alloc, ref);
    defer alloc.free(source_markdown);

    // The rendered skill.md is ours, so a parse/validate failure is a
    // programming error surfaced loud — not untrusted input.
    var skill_meta = fleet_config.parseSkillMetadata(alloc, source_markdown) catch return error.ResidentSkillInvalid;
    defer skill_meta.deinit(alloc);

    const trigger_markdown = try buildReactiveTrigger(alloc, skill_meta.name);
    defer alloc.free(trigger_markdown);
    var parsed = fleet_config.parseTriggerMarkdownWithJson(alloc, trigger_markdown) catch return error.ResidentConfigInvalid;
    defer parsed.deinit(alloc);
    try assertReactiveReadonly(parsed.config); // Invariant 2

    const fleet_id = try id_format.generateFleetId(alloc);
    var keep = false;
    defer if (!keep) alloc.free(fleet_id);

    const no_tags = [_][]const u8{};
    fleet_row.insertFleetOnConn(conn, workspace_id, source_markdown, trigger_markdown, parsed, &no_tags, null, fleet_id, clock.nowMillis()) catch |err| {
        // A concurrent same-channel first-mention (or a prior partial
        // materialization) already took this channel's unique fleet name →
        // converge on that fleet instead of wedging on the constraint.
        if (fleet_row.isUniqueViolation(conn))
            return resolveExistingByName(alloc, conn, queue, workspace_id, skill_meta.name, team_id, channel_id);
        return err;
    };

    // Stream + consumer group before the binding (the lease XREADGROUP needs
    // it). On failure roll the row back so the next mention re-materializes
    // cleanly rather than wedging on the now-taken name.
    create_stream.ensureEventStream(queue, fleet_id) catch |err| {
        fleet_row.deleteFleetRow(conn, workspace_id, fleet_id) catch |re|
            log.warn("channel_fleet_rollback_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = fleet_id, .err = @errorName(re) });
        return err;
    };
    // Activate BEFORE writing the binding so the invariant "a binding exists ⇒ the
    // fleet is leaseable" holds. A reactive resident fleet has no provisioning beat
    // (the install-steps beat is dashboard-only cosmetics), so it is leaseable the
    // instant its row + stream exist — the runner leases only `active` fleets. The
    // flip is idempotent (guarded installing→active). On failure we return the
    // error WITHOUT binding; the leftover installing row is reclaimed by the next
    // mention's unique-violation convergence, which retries the flip.
    try fleet_row.activateFleetOnConn(conn, workspace_id, fleet_id, clock.nowMillis());
    try insertBinding(alloc, conn, team_id, channel_id, fleet_id);

    log.info("channel_fleet_materialized", .{ .team_id = team_id, .channel_id = channel_id, .fleet_id = fleet_id });
    keep = true;
    return fleet_id;
}

/// Converge on the fleet another mention already created for this channel
/// (found by its unique name). Idempotent stream-ensure + binding-upsert cover
/// a winner that failed between insert and its own stream/binding steps.
fn resolveExistingByName(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    queue: *queue_redis.Client,
    workspace_id: []const u8,
    name: []const u8,
    team_id: []const u8,
    channel_id: []const u8,
) ![]const u8 {
    const fleet_id = (try selectFleetByName(alloc, conn, workspace_id, name)) orelse return error.ResidentFleetVanished;
    errdefer alloc.free(fleet_id);
    try create_stream.ensureEventStream(queue, fleet_id);
    // Idempotent activation on the convergence path too: the race winner may not
    // have flipped yet (or failed after insert), so the loser guarantees the
    // shared fleet is leaseable before it writes the binding (guarded → 0-row
    // no-op if the winner already flipped).
    try fleet_row.activateFleetOnConn(conn, workspace_id, fleet_id, clock.nowMillis());
    try insertBinding(alloc, conn, team_id, channel_id, fleet_id);
    return fleet_id;
}

fn selectBinding(alloc: std.mem.Allocator, conn: *pg.Conn, team_id: []const u8, channel_id: []const u8) !?[]const u8 {
    var q = PgQuery.from(try conn.query(sql.SELECT_CHANNEL_FLEET, .{ spec.PROVIDER, team_id, channel_id }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    return try alloc.dupe(u8, try row.get([]const u8, 0));
}

fn selectFleetByName(alloc: std.mem.Allocator, conn: *pg.Conn, workspace_id: []const u8, name: []const u8) !?[]const u8 {
    var q = PgQuery.from(try conn.query(sql.SELECT_FLEET_BY_NAME, .{ workspace_id, name }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    return try alloc.dupe(u8, try row.get([]const u8, 0));
}

fn insertBinding(alloc: std.mem.Allocator, conn: *pg.Conn, team_id: []const u8, channel_id: []const u8, fleet_id: []const u8) !void {
    const uid = try id_format.generateConnectorChannelId(alloc);
    defer alloc.free(uid);
    _ = try conn.exec(sql.INSERT_CHANNEL_BINDING, .{ uid, spec.PROVIDER, team_id, channel_id, fleet_id, KIND_RESIDENT, clock.nowMillis() });
}

/// The reactive config (Invariant 2), built in code — one parameterless `api`
/// trigger (woken only by an XADD to the fleet's stream), no tools, a code-set
/// budget. Never parsed from skill.md prose. `name` must equal the skill.md
/// `name:` (the create-path cross-file invariant).
fn buildReactiveTrigger(alloc: std.mem.Allocator, name: []const u8) ![]const u8 {
    return std.fmt.allocPrint(alloc,
        \\---
        \\name: {s}
        \\x-agentsfleet:
        \\  triggers:
        \\    - type: api
        \\  tools: []
        \\  budget:
        \\    daily_dollars: {s}
        \\---
        \\
    , .{ name, RESIDENT_DAILY_DOLLARS });
}

/// Assert the built config is reactive + read-only (Invariant 2): exactly one
/// `api` trigger (no `webhook`/`cron` autonomous trigger) and no tools. The
/// reactive shape is the lone `api` trigger — an empty `triggers` slice is
/// rejected by the parser, so "reactive" is a present `api` trigger, not the
/// absence of one.
fn assertReactiveReadonly(cfg: fleet_config.FleetConfig) !void {
    if (cfg.triggers.len != 1) return error.ResidentPolicyViolation;
    switch (cfg.triggers[0]) {
        .api => {},
        .webhook, .cron => return error.ResidentPolicyViolation,
    }
    if (cfg.tools.len != 0) return error.ResidentPolicyViolation;
}

fn renderSkill(alloc: std.mem.Allocator, ref: []const u8) ![]const u8 {
    const size = std.mem.replacementSize(u8, DEFAULT_SKILL, CHANNEL_REF_PLACEHOLDER, ref);
    const out = try alloc.alloc(u8, size);
    _ = std.mem.replace(u8, DEFAULT_SKILL, CHANNEL_REF_PLACEHOLDER, ref, out);
    return out;
}

/// `<lowercased team_id>-<lowercased channel_id>` — the per-(team,channel)
/// slug that keys the resident fleet's name (a valid `^[a-z0-9-]+$` skill name).
fn channelRef(alloc: std.mem.Allocator, team_id: []const u8, channel_id: []const u8) ![]const u8 {
    const t = try std.ascii.allocLowerString(alloc, team_id);
    defer alloc.free(t);
    const c = try std.ascii.allocLowerString(alloc, channel_id);
    defer alloc.free(c);
    return std.fmt.allocPrint(alloc, "{s}-{s}", .{ t, c });
}

// ── Tests (Dim 3.3: the resident config is reactive/read-only, code-built) ───

const testing = std.testing;

test "buildReactiveTrigger → parses to exactly one api trigger, no tools" {
    const alloc = testing.allocator;
    const tm = try buildReactiveTrigger(alloc, "slack-channel-c024be7lh");
    defer alloc.free(tm);
    var parsed = try fleet_config.parseTriggerMarkdownWithJson(alloc, tm);
    defer parsed.deinit(alloc);
    try testing.expectEqual(@as(usize, 1), parsed.config.triggers.len);
    try testing.expect(parsed.config.triggers[0] == .api);
    try testing.expectEqual(@as(usize, 0), parsed.config.tools.len);
    try assertReactiveReadonly(parsed.config); // the code-built config passes the guard
}

test "assertReactiveReadonly rejects a webhook trigger (no autonomous wake)" {
    const alloc = testing.allocator;
    const tm =
        \\---
        \\name: sneaky
        \\x-agentsfleet:
        \\  triggers:
        \\    - type: webhook
        \\      source: github
        \\  tools: []
        \\  budget:
        \\    daily_dollars: 1.0
        \\---
    ;
    var parsed = try fleet_config.parseTriggerMarkdownWithJson(alloc, tm);
    defer parsed.deinit(alloc);
    try testing.expectError(error.ResidentPolicyViolation, assertReactiveReadonly(parsed.config));
}

test "assertReactiveReadonly rejects a write tool slipping into a resident" {
    const alloc = testing.allocator;
    const tm =
        \\---
        \\name: sneaky
        \\x-agentsfleet:
        \\  triggers:
        \\    - type: api
        \\  tools:
        \\    - git
        \\  budget:
        \\    daily_dollars: 1.0
        \\---
    ;
    var parsed = try fleet_config.parseTriggerMarkdownWithJson(alloc, tm);
    defer parsed.deinit(alloc);
    try testing.expectError(error.ResidentPolicyViolation, assertReactiveReadonly(parsed.config));
}

test "channelRef lowercases + joins team and channel into a valid name slug" {
    const alloc = testing.allocator;
    const ref = try channelRef(alloc, "T024BE7LH", "C061EG9");
    defer alloc.free(ref);
    try testing.expectEqualStrings("t024be7lh-c061eg9", ref);
}

test "renderSkill substitutes the channel-ref placeholder into the fleet name" {
    const alloc = testing.allocator;
    const rendered = try renderSkill(alloc, "t024be7lh-c061eg9");
    defer alloc.free(rendered);
    try testing.expect(std.mem.indexOf(u8, rendered, "name: slack-channel-t024be7lh-c061eg9") != null);
    try testing.expect(std.mem.indexOf(u8, rendered, CHANNEL_REF_PLACEHOLDER) == null);
    // The rendered skill must parse + validate (name slug + required fields).
    var meta = try fleet_config.parseSkillMetadata(alloc, rendered);
    defer meta.deinit(alloc);
    try testing.expectEqualStrings("slack-channel-t024be7lh-c061eg9", meta.name);
}
