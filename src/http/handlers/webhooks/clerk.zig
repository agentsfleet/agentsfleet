//! Clerk signup webhook handler — `POST /v1/webhooks/clerk`.
//!
//! Verifies an Svix-signed `user.created` event against env
//! `CLERK_WEBHOOK_SECRET`, parses the payload, and atomically provisions a
//! personal account via `signup_bootstrap.bootstrapPersonalAccount`.
//! Idempotent on `oidc_subject` — replays return `created:false`.
//!
//! Error surface:
//!   401 UZ-WH-010 — invalid Svix signature (also fires if secret missing)
//!   401 UZ-WH-011 — stale Svix timestamp (>5min drift)
//!   400 UZ-REQ-001 — malformed JSON or missing primary email
//!   413 UZ-REQ-002 — body >2MB
//!   500 UZ-INTERNAL-00X — DB error or misconfig
//!
//! Missing `CLERK_WEBHOOK_SECRET` fails with 500 (operator misconfig), not
//! 401 — responding 401 would leak "no secret configured" to attackers.

const std = @import("std");
const httpz = @import("httpz");

const hx_mod = @import("../hx.zig");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");
const svix_verify = @import("../../../crypto/svix_verify.zig");
const signup_bootstrap = @import("../../../state/signup_bootstrap.zig");
const metrics = @import("../../../observability/metrics_counters.zig");
const telemetry_mod = @import("../../../observability/telemetry.zig");

const log = std.log.scoped(.clerk_webhook);

const Hx = hx_mod.Hx;

/// Clerk's `user.created` payload, tolerant of unknown fields. See
/// https://clerk.com/docs/users/webhooks#user-created.
const ClerkEmailAddress = struct {
    id: []const u8,
    email_address: []const u8,
};

const ClerkUserData = struct {
    id: []const u8,
    email_addresses: []const ClerkEmailAddress = &.{},
    primary_email_address_id: ?[]const u8 = null,
    first_name: ?[]const u8 = null,
    last_name: ?[]const u8 = null,
};

const ClerkEvent = struct {
    type: []const u8,
    data: ClerkUserData,
};

pub fn innerClerkWebhook(hx: Hx, req: *httpz.Request) void {
    const secret = readSecret(hx) orelse return;
    defer std.heap.page_allocator.free(secret);

    const svix_id = req.header(svix_verify.SVIX_ID_HEADER) orelse {
        rejectBadSig(hx, "missing svix-id header");
        return;
    };
    const svix_ts = req.header(svix_verify.SVIX_TS_HEADER) orelse {
        rejectBadSig(hx, "missing svix-timestamp header");
        return;
    };
    const svix_sig = req.header(svix_verify.SVIX_SIG_HEADER) orelse {
        rejectBadSig(hx, "missing svix-signature header");
        return;
    };

    const body = req.body() orelse {
        rejectMissingEmail(hx, "empty request body");
        return;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;

    const verify_result = svix_verify.verifySvix(
        secret,
        svix_id,
        svix_ts,
        svix_sig,
        body,
        std.time.timestamp(),
        svix_verify.SVIX_MAX_DRIFT_SECONDS,
    );
    switch (verify_result) {
        .ok => {},
        .invalid_signature => {
            rejectBadSig(hx, "svix signature verification failed");
            return;
        },
        .stale_timestamp => {
            rejectStaleTs(hx);
            return;
        },
    }

    const parsed = std.json.parseFromSlice(ClerkEvent, hx.alloc, body, .{
        .ignore_unknown_fields = true,
    }) catch {
        rejectMissingEmail(hx, "malformed clerk event json");
        return;
    };
    defer parsed.deinit();

    const event = parsed.value;
    // Only user.created is in scope. Other event types (user.updated,
    // user.deleted) are ignored with 200 so Clerk stops retrying.
    if (!std.mem.eql(u8, event.type, "user.created")) {
        log.info("clerk.event_ignored type={s} req_id={s}", .{ event.type, hx.req_id });
        hx.ok(.ok, .{ .status = "ignored", .type = event.type });
        return;
    }

    const primary_email = pickPrimaryEmail(event.data) orelse {
        rejectMissingEmail(hx, "no primary email address on clerk event");
        return;
    };
    const display_name = deriveDisplayName(hx.alloc, event.data.first_name, event.data.last_name);
    defer if (display_name) |d| hx.alloc.free(d);

    runBootstrap(hx, event.data.id, primary_email, display_name);
}

// ── Response helpers ──────────────────────────────────────────────────────

fn rejectBadSig(hx: Hx, detail: []const u8) void {
    log.warn("clerk.bad_sig detail=\"{s}\" req_id={s}", .{ detail, hx.req_id });
    metrics.incSignupFailed(.bad_sig);
    hx.fail(ec.ERR_WEBHOOK_SIG_INVALID, detail);
}

fn rejectStaleTs(hx: Hx) void {
    log.warn("clerk.stale_ts req_id={s}", .{hx.req_id});
    metrics.incSignupFailed(.stale_ts);
    hx.fail(ec.ERR_WEBHOOK_TIMESTAMP_STALE, "Clerk webhook timestamp outside freshness window");
}

fn rejectMissingEmail(hx: Hx, detail: []const u8) void {
    log.warn("clerk.bad_request detail=\"{s}\" req_id={s}", .{ detail, hx.req_id });
    metrics.incSignupFailed(.missing_email);
    hx.fail(ec.ERR_INVALID_REQUEST, detail);
}

// ── Secret read ───────────────────────────────────────────────────────────

/// Missing or empty CLERK_WEBHOOK_SECRET → 500 (operator misconfig, not 401 —
/// we don't want to confirm to attackers that the endpoint has no secret
/// configured). Uses page_allocator so the slice can outlive arenas.
fn readSecret(hx: Hx) ?[]u8 {
    const secret = std.process.getEnvVarOwned(std.heap.page_allocator, "CLERK_WEBHOOK_SECRET") catch {
        log.err("clerk.secret_missing req_id={s}", .{hx.req_id});
        common.internalOperationError(hx.res, "CLERK_WEBHOOK_SECRET not configured", hx.req_id);
        return null;
    };
    if (secret.len == 0) {
        std.heap.page_allocator.free(secret);
        log.err("clerk.secret_empty req_id={s}", .{hx.req_id});
        common.internalOperationError(hx.res, "CLERK_WEBHOOK_SECRET is empty", hx.req_id);
        return null;
    }
    return secret;
}

// ── Payload helpers ───────────────────────────────────────────────────────

fn pickPrimaryEmail(data: ClerkUserData) ?[]const u8 {
    const primary_id = data.primary_email_address_id orelse {
        if (data.email_addresses.len == 0) return null;
        return data.email_addresses[0].email_address;
    };
    for (data.email_addresses) |addr| {
        if (std.mem.eql(u8, addr.id, primary_id)) return addr.email_address;
    }
    return null;
}

fn deriveDisplayName(alloc: std.mem.Allocator, first: ?[]const u8, last: ?[]const u8) ?[]u8 {
    const f = trimOrNull(first);
    const l = trimOrNull(last);
    if (f == null and l == null) return null;
    if (f != null and l != null) {
        return std.fmt.allocPrint(alloc, "{s} {s}", .{ f.?, l.? }) catch null;
    }
    const single = if (f) |s| s else l.?;
    return alloc.dupe(u8, single) catch null;
}

fn trimOrNull(s: ?[]const u8) ?[]const u8 {
    const raw = s orelse return null;
    const trimmed = std.mem.trim(u8, raw, " \t\r\n");
    if (trimmed.len == 0) return null;
    return trimmed;
}

// ── Bootstrap invocation ──────────────────────────────────────────────────

fn runBootstrap(hx: Hx, oidc_subject: []const u8, email: []const u8, display_name: ?[]const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        log.err("clerk.pool_acquire_failed req_id={s}", .{hx.req_id});
        metrics.incSignupFailed(.db_error);
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    var bootstrap = signup_bootstrap.bootstrapPersonalAccount(
        conn,
        hx.alloc,
        .{
            .oidc_subject = oidc_subject,
            .email = email,
            .display_name = display_name,
        },
    ) catch |err| {
        log.err("clerk.bootstrap_failed oidc={s} err={s} req_id={s}", .{ oidc_subject, @errorName(err), hx.req_id });
        metrics.incSignupFailed(.db_error);
        common.internalOperationError(hx.res, "Signup bootstrap failed", hx.req_id);
        return;
    };
    defer bootstrap.deinit(hx.alloc);

    captureSignupEvent(hx, oidc_subject, email, bootstrap);
    hx.ok(.ok, .{
        .workspace_id = bootstrap.workspace_id,
        .workspace_name = bootstrap.workspace_name,
        .created = bootstrap.created,
    });
}

fn captureSignupEvent(hx: Hx, oidc_subject: []const u8, email: []const u8, bootstrap: signup_bootstrap.Bootstrap) void {
    const at = std.mem.indexOfScalar(u8, email, '@') orelse email.len;
    const email_domain = if (at + 1 < email.len) email[at + 1 ..] else "";
    hx.ctx.telemetry.capture(telemetry_mod.SignupBootstrapped, .{
        .distinct_id = oidc_subject,
        .tenant_id = bootstrap.tenant_id,
        .workspace_id = bootstrap.workspace_id,
        .workspace_name = bootstrap.workspace_name,
        .email_domain = email_domain,
        .created = bootstrap.created,
        .request_id = hx.req_id,
    });
}
