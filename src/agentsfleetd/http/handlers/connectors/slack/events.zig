//! POST /v1/connectors/slack/events — Slack Events API ingress.
//!
//! Auth: Slack v0 request signature ONLY (no Bearer — mirrors the webhook
//! plane, Invariant 5). The platform signing secret lives in the
//! admin-workspace `slack-app` vault entry (decision: connector data-secrets
//! live in the vault, keyed by `Context.platform_admin_workspace_id`, no
//! boot/env wiring) and is cached once-per-process on `Context` after the
//! first successful read — steady state does the vault SELECT zero times,
//! while an unconfigured deployment keeps re-reading until the operator
//! vaults the secret, so live vaulting still needs no restart.
//!
//! Flow (all inline, ≤3 s — there is no deferred-task substrate; the model run
//! is the runner's async job, not this handler's):
//!   0. header presence (cheap, DB-free) → reject before any acquire.
//!   1. resolve the signing secret: the process cache, or (first request /
//!      unconfigured) one pool acquire + `slack-app` vault SELECT that
//!      publishes into the cache.
//!   2. verify signature (constant-time, 300 s window) → UZ-SLK-010 /
//!      UZ-SLK-011. With the cache warm, a request with wrong sig/ts headers
//!      is rejected with ZERO database work; `url_verification` and ignored
//!      shapes never acquire a conn at all.
//!   3. `url_verification` → echo the challenge.
//!   4. resolve team_id → workspace (`connector_installs`); unknown team is a
//!      200-ack no-op (UZ-SLK-020 — Slack must never see an error loop). The
//!      conn is acquired here (mention path only) when step 1 didn't already.
//!   5. resolve (team, channel) → resident fleet (materialize on miss).
//!   6. dedup on (channel_fleet_id, event.ts) + XADD a `slack:<user>` chat event
//!      onto `fleet:{channel_fleet_id}:events` (the webhook-producer shape, no
//!      principal). The runner leases + answers later via chat.postMessage (§4).

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");
const constants = @import("common");
const clock = constants.clock;

const common = @import("../../common.zig");
const hx_mod = @import("../../hx.zig");
const ec = @import("../../../../errors/error_registry.zig");
const PgQuery = @import("../../../../db/pg_query.zig").PgQuery;
const EventEnvelope = @import("contract").event_envelope;
const oauth2 = @import("../oauth2.zig");
const slack_sig = @import("slack_sig.zig");
const event_parse = @import("event_parse.zig");
const channel_fleet = @import("channel_fleet.zig");
const post = @import("post.zig");
const thread = @import("thread.zig");
const spec = @import("spec.zig");

const log = logging.scoped(.connector_slack);

const Hx = hx_mod.Hx;
const Mention = event_parse.Mention;

const F_SIGNING_SECRET = "signing_secret";
const S_SIG_DETAIL = "Slack signature verification failed";
const S_STALE_DETAIL = "Slack request timestamp is outside the 300s window";
const S_NOT_CONFIGURED = "Slack signing secret is not configured";
// Structured log event names (RULE UFS — each used at ≥2 rejection/ignore sites).
const EV_INGRESS_REJECTED = "slack_ingress_rejected";
const EV_INGRESS_IGNORED = "slack_ingress_ignored";

const SELECT_INSTALL_SQL =
    \\SELECT workspace_id::text FROM core.connector_installs
    \\WHERE provider = $1 AND external_account_id = $2
;

pub fn innerSlackEvents(hx: Hx, req: *httpz.Request) void {
    const body = req.body() orelse "";
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;

    const provided_sig = req.header(slack_sig.SIG_HEADER) orelse {
        rejectSig(hx, ec.ERR_SLACK_SIG_INVALID, "missing_signature", S_SIG_DETAIL);
        return;
    };
    const timestamp = req.header(slack_sig.TS_HEADER) orelse {
        // A missing timestamp header is unverifiable (no basestring, no freshness
        // input) → invalid signature, not "stale" (which implies a present-but-old ts).
        rejectSig(hx, ec.ERR_SLACK_SIG_INVALID, "missing_timestamp", S_SIG_DETAIL);
        return;
    };

    // Lazily-acquired conn: the cached-secret steady state verifies + parses
    // without one; only the mention path (and the one-time cache fill) pays
    // for a pool acquire.
    var maybe_conn: ?*pg.Conn = null;
    defer if (maybe_conn) |c| hx.ctx.pool.release(c);

    const signing_secret = resolveSigningSecret(hx, &maybe_conn) orelse return;

    switch (slack_sig.verify(signing_secret, timestamp, provided_sig, body)) {
        .ok => {},
        .bad_signature => return rejectSig(hx, ec.ERR_SLACK_SIG_INVALID, "invalid_signature", S_SIG_DETAIL),
        .stale_timestamp => return rejectSig(hx, ec.ERR_SLACK_TIMESTAMP_STALE, "stale_timestamp", S_STALE_DETAIL),
    }

    // Signature verified — safe to parse + act on the body. A signed-but-
    // unparseable body is 200-acked (not 4xx): the sender is already
    // authenticated, and an error status would make Slack retry-loop the same
    // delivery — consistent with `parseSlackEvent` degrading unknown shapes to
    // `.ignore` (RULE OBS: the rejection is logged either way).
    var parsed = std.json.parseFromSlice(std.json.Value, hx.alloc, body, .{}) catch {
        log.warn(EV_INGRESS_IGNORED, .{ .reason = "unparseable_body", .error_code = ec.ERR_INVALID_REQUEST });
        hx.ok(.ok, .{ .status = ec.STATUS_ACCEPTED });
        return;
    };
    defer parsed.deinit();
    const root = switch (parsed.value) {
        .object => |o| o,
        else => {
            log.warn(EV_INGRESS_IGNORED, .{ .reason = "non_object_body", .error_code = ec.ERR_INVALID_REQUEST });
            hx.ok(.ok, .{ .status = ec.STATUS_ACCEPTED });
            return;
        },
    };

    switch (event_parse.parseSlackEvent(root)) {
        .url_verification => |challenge| hx.ok(.ok, .{ .challenge = challenge }),
        .ignore => hx.ok(.ok, .{ .status = ec.STATUS_ACCEPTED }),
        .app_mention => |m| {
            if (maybe_conn == null) {
                maybe_conn = hx.ctx.pool.acquire() catch {
                    common.internalDbUnavailable(hx.res, hx.req_id);
                    return;
                };
            }
            dispatchMention(hx, &maybe_conn, m);
        },
    }
}

/// Resolve the platform signing secret: the process-lifetime cached slice when
/// present; otherwise one vault read (acquiring the request's conn on the way,
/// handed back via `maybe_conn` for the caller's defer) that publishes into the
/// Context cache. Emits the failure response and returns null when the secret
/// is unconfigured (fail loud, UZ-CONN-001 — the operator must vault it via the
/// registration playbook) or when the publish allocation failed.
fn resolveSigningSecret(hx: Hx, maybe_conn: *?*pg.Conn) ?[]const u8 {
    if (hx.ctx.cachedSlackSigningSecret()) |s| return s;
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return null;
    };
    maybe_conn.* = conn;
    const loaded = loadSigningSecret(hx.ctx.alloc, conn, hx.ctx.platform_admin_workspace_id) orelse {
        log.err("slack_signing_secret_missing", .{ .error_code = ec.ERR_CONNECTOR_NOT_CONFIGURED });
        hx.fail(ec.ERR_CONNECTOR_NOT_CONFIGURED, S_NOT_CONFIGURED);
        return null;
    };
    return hx.ctx.publishSlackSigningSecret(loaded) orelse {
        common.internalOperationError(hx.res, "Failed to prepare Slack request verification", hx.req_id);
        return null;
    };
}

/// Resolve the install + resident fleet, pre-load the bot token, RELEASE the
/// pooled conn, then enqueue. An unknown team is a 200-ack no-op (UZ-SLK-020):
/// the workspace uninstalled or never installed, and an error status would
/// make Slack retry-loop. Takes the caller's conn SLOT (`maybe_conn`, non-null
/// on entry) so the release happens here, before any vendor HTTP — a stalled
/// vendor must degrade this one mention, never park a pool slot with it.
fn dispatchMention(hx: Hx, maybe_conn: *?*pg.Conn, m: Mention) void {
    const conn = maybe_conn.*.?;
    const workspace_id = resolveWorkspace(hx.alloc, conn, m.team_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        log.warn(EV_INGRESS_REJECTED, .{ .reason = "team_not_installed", .error_code = ec.ERR_SLACK_TEAM_NOT_INSTALLED, .team_id = m.team_id });
        hx.ok(.ok, .{ .ignored = ec.ERR_SLACK_TEAM_NOT_INSTALLED });
        return;
    };
    defer hx.alloc.free(workspace_id);

    const channel_fleet_id = channel_fleet.resolveOrCreate(hx.alloc, conn, hx.ctx.queue, workspace_id, m.team_id, m.channel) catch |err| {
        log.err("slack_channel_fleet_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .team_id = m.team_id, .channel_id = m.channel, .err = @errorName(err) });
        common.internalOperationError(hx.res, "Failed to resolve the fleet for this Slack channel", hx.req_id);
        return;
    };
    defer hx.alloc.free(channel_fleet_id);

    // Pre-load the bot token for the best-effort thread re-read — the last
    // database read this request makes. Absent/garbled handle → null → the
    // re-read degrades to an empty thread downstream.
    const bot_token: ?[]const u8 = post.loadBotToken(hx.alloc, conn, workspace_id) catch null;
    defer if (bot_token) |t| hx.alloc.free(t);

    // Everything after this line is Redis + vendor HTTP: hand the pool slot
    // back NOW instead of riding it through a (deadline-armed, but still up to
    // 1.5 s) vendor read.
    hx.ctx.pool.release(conn);
    maybe_conn.* = null;

    enqueueMention(hx, bot_token, workspace_id, channel_fleet_id, m);
}

/// Dedup on (channel_fleet_id, event.ts) then XADD the mention as a `slack:<user>`
/// chat event — the no-principal webhook-producer shape. The dedup slot is
/// released on any post-claim failure so Slack's retry stays deliverable.
/// Conn-free by design: runs entirely on Redis + the deadline-armed vendor read.
fn enqueueMention(hx: Hx, bot_token: ?[]const u8, workspace_id: []const u8, channel_fleet_id: []const u8, m: Mention) void {
    var dedup_buf: [256]u8 = undefined;
    const dedup_key = std.fmt.bufPrint(&dedup_buf, "{s}{s}:{s}", .{ ec.SLACK_DEDUP_KEY_PREFIX, channel_fleet_id, m.ts }) catch {
        common.internalOperationError(hx.res, "Failed to build the duplicate-event check", hx.req_id);
        return;
    };
    const is_new = hx.ctx.queue.setNx(dedup_key, "1", ec.DEDUP_TTL_SECONDS) catch |err| {
        log.err("slack_dedup_error", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .channel_fleet_id = channel_fleet_id, .err = @errorName(err) });
        common.internalOperationError(hx.res, "Failed to check for a duplicate event", hx.req_id);
        return;
    };
    if (!is_new) {
        hx.ok(.ok, .{ .status = ec.STATUS_DUPLICATE });
        return;
    }

    // Best-effort recent-thread re-read (bounded last-N, deadline-armed) so the
    // runner's input carries same-thread continuity the mention-only bot is
    // otherwise blind to. Any failure degrades to an empty thread (the answer
    // still works from durable channel memory + the mention text); dedup on
    // event.ts makes a slow Slack call safe to retry.
    const api_base = hx.ctx.connector_slack_api_base_override orelse post.SLACK_API_BASE_DEFAULT;
    var recent = thread.fetchRecent(hx.alloc, hx.ctx.io, hx.ctx.deadline_scheduler, bot_token, api_base, workspace_id, m.channel, m.thread_ts orelse m.ts);
    defer recent.deinit();

    const request_json = buildRequestJson(hx.alloc, m, recent.msgs) catch {
        releaseDedup(hx, channel_fleet_id, dedup_key);
        common.internalOperationError(hx.res, "Failed to prepare the mention for delivery", hx.req_id);
        return;
    };
    defer hx.alloc.free(request_json);

    var actor_buf: [128]u8 = undefined;
    const actor = std.fmt.bufPrint(&actor_buf, "{s}{s}", .{ constants.SLACK_ACTOR_PREFIX, m.user }) catch "slack:unknown";

    const envelope = EventEnvelope{
        .event_id = "",
        .fleet_id = channel_fleet_id,
        .workspace_id = workspace_id,
        .actor = actor,
        .event_type = .chat,
        .request_json = request_json,
        .created_at = clock.nowMillis(),
    };
    const new_event_id = hx.ctx.queue.xaddFleetEvent(envelope) catch |err| {
        releaseDedup(hx, channel_fleet_id, dedup_key);
        log.err("slack_enqueue_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .channel_fleet_id = channel_fleet_id, .err = @errorName(err) });
        common.internalOperationError(hx.res, "Failed to enqueue mention", hx.req_id);
        return;
    };
    defer hx.ctx.alloc.free(new_event_id);

    log.debug("slack_mention_enqueued", .{ .channel_fleet_id = channel_fleet_id, .stream_event_id = new_event_id, .actor = actor });
    hx.ok(.ok, .{ .status = ec.STATUS_ACCEPTED });
}

/// `{ text, reply_thread_ts, channel_id, recent_thread_msgs }` — the runner's
/// input for the answer. `reply_thread_ts = thread_ts orelse ts` so a top-level
/// mention anchors its own thread (the reply is always threaded, never a
/// detached channel post). `recent_thread_msgs` is the bounded last-N thread
/// re-read (§4 E) — `[]` when the re-read degraded (best-effort).
fn buildRequestJson(alloc: std.mem.Allocator, m: Mention, recent_msgs: []const thread.Msg) ![]const u8 {
    return std.json.Stringify.valueAlloc(alloc, .{
        .text = m.text,
        .reply_thread_ts = m.thread_ts orelse m.ts,
        .channel_id = m.channel,
        .recent_thread_msgs = recent_msgs,
    }, .{});
}

/// Release a claimed idempotency slot after a post-claim failure so Slack's
/// retry of this delivery is not answered "duplicate" for an event that never
/// landed. Best-effort — on a DEL failure the slot expires at its TTL.
fn releaseDedup(hx: Hx, channel_fleet_id: []const u8, dedup_key: []const u8) void {
    hx.ctx.queue.del(dedup_key) catch |err|
        log.warn("slack_dedup_release_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .channel_fleet_id = channel_fleet_id, .err = @errorName(err) });
}

fn rejectSig(hx: Hx, code: []const u8, reason: []const u8, detail: []const u8) void {
    log.warn(EV_INGRESS_REJECTED, .{ .reason = reason, .error_code = code });
    hx.fail(code, detail);
}

fn resolveWorkspace(alloc: std.mem.Allocator, conn: *pg.Conn, team_id: []const u8) !?[]const u8 {
    var q = PgQuery.from(try conn.query(SELECT_INSTALL_SQL, .{ spec.PROVIDER, team_id }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    return try alloc.dupe(u8, try row.get([]const u8, 0));
}

/// Resolve the platform Slack signing secret from the admin-workspace
/// `slack-app` vault entry — the same entry `oauth2.loadAppCreds` reads for the
/// client id/secret; this reads the `signing_secret` field via the shared
/// `oauth2.loadAppVaultJson` loader (RULE UFS — one site builds the `-app` key).
/// Null = unconfigured/missing → the handler fails closed.
fn loadSigningSecret(alloc: std.mem.Allocator, conn: *pg.Conn, admin_ws_id: []const u8) ?[]const u8 {
    var parsed = oauth2.loadAppVaultJson(alloc, conn, admin_ws_id, spec.PROVIDER) orelse return null;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return null,
    };
    const s = switch (obj.get(F_SIGNING_SECRET) orelse return null) {
        .string => |v| v,
        else => return null,
    };
    return alloc.dupe(u8, s) catch return null;
}

test {
    _ = event_parse;
    _ = channel_fleet;
    _ = slack_sig;
    _ = thread;
}
