//! Clerk `user.created` auth-plane handler — `POST /v1/auth/identity-events/clerk`.
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
const constants = @import("common");
const clock = constants.clock;
const httpz = @import("httpz");
const logging = @import("log");

const hx_mod = @import("../hx.zig");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");
const svix_verify = @import("../../../auth/crypto/svix_verify.zig");
const signup_bootstrap = @import("../../../state/signup_bootstrap.zig");
const account_teardown = @import("../../../state/account_teardown.zig");
const metrics = @import("../../../observability/metrics_counters.zig");
const telemetry_mod = @import("../../../observability/telemetry.zig");
const clerk_backend = @import("../../../auth/clerk_backend.zig");
const scopes = @import("../../../auth/scopes.zig");

/// Default capability `scopes` written to Clerk publicMetadata on signup
/// the `tenant` default grant — every tenant capability, so a new
/// owner can create workspaces + fleets with no manual step. The JWT template
/// reads `public_metadata.scopes` into the `scopes` claim. Platform-operator
/// scopes (`workspace:any`, `runner:enroll`, …) are NEVER auto-granted — they
/// are a manual Clerk edit on the operator user.
const DEFAULT_SIGNUP_SCOPES = scopes.defaultClaim(.tenant);

const log = logging.scoped(.auth_identity_events);

// Deliberately generic — see readSecret()'s missing/empty branches: a distinct
// code or detail here would confirm to an unauthenticated caller that this
// deployment has no CLERK_WEBHOOK_SECRET configured.
const S_SECRET_CHECK_FAILED = "Failed to verify this request";

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
        clock.nowSeconds(),
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
    if (std.mem.eql(u8, event.type, "user.deleted")) {
        runDelete(hx, event.data.id);
        return;
    }
    if (!std.mem.eql(u8, event.type, "user.created")) {
        log.debug("event_ignored", .{ .type = event.type, .req_id = hx.req_id });
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
    log.warn("bad_sig", .{
        .error_code = ec.ERR_WEBHOOK_SIG_INVALID,
        .detail = detail,
        .req_id = hx.req_id,
    });
    metrics.incSignupFailed(.bad_sig);
    hx.fail(ec.ERR_WEBHOOK_SIG_INVALID, detail);
}

fn rejectStaleTs(hx: Hx) void {
    log.warn("stale_ts", .{
        .error_code = ec.ERR_WEBHOOK_TIMESTAMP_STALE,
        .req_id = hx.req_id,
    });
    metrics.incSignupFailed(.stale_ts);
    hx.fail(ec.ERR_WEBHOOK_TIMESTAMP_STALE, "Clerk webhook timestamp outside freshness window");
}

fn rejectMissingEmail(hx: Hx, detail: []const u8) void {
    log.warn("bad_request", .{
        .error_code = ec.ERR_INVALID_REQUEST,
        .detail = detail,
        .req_id = hx.req_id,
    });
    metrics.incSignupFailed(.missing_email);
    hx.fail(ec.ERR_INVALID_REQUEST, detail);
}

// ── Secret read ───────────────────────────────────────────────────────────

/// Missing or empty CLERK_WEBHOOK_SECRET → 500 (operator misconfig, not 401 —
/// we don't want to confirm to attackers that the endpoint has no secret
/// configured). Borrows the boot-resolved Context secret (no per-request read).
fn readSecret(hx: Hx) ?[]const u8 {
    // Log at warn so the negative-path test doesn't trip "logged errors" test
    // gates. The user-visible signal is the 500 response + 5xx-rate Prometheus
    // alerting on /v1/auth/identity-events/clerk — the log line is supporting
    // context, not the primary alert.
    const secret = hx.ctx.clerk_webhook_secret orelse {
        log.warn("secret_missing", .{
            .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
            .req_id = hx.req_id,
        });
        common.internalOperationError(hx.res, S_SECRET_CHECK_FAILED, hx.req_id); // mudball-ok: pre-auth path, see S_SECRET_CHECK_FAILED's doc comment
        return null;
    };
    if (secret.len == 0) {
        log.warn("secret_empty", .{
            .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
            .req_id = hx.req_id,
        });
        common.internalOperationError(hx.res, S_SECRET_CHECK_FAILED, hx.req_id); // mudball-ok: pre-auth path, see S_SECRET_CHECK_FAILED's doc comment
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
        log.warn("pool_acquire_failed", .{
            .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE,
            .req_id = hx.req_id,
        });
        metrics.incSignupFailed(.pool_unavailable);
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
        log.warn("bootstrap_failed", .{
            .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
            .oidc = oidc_subject,
            .err = @errorName(err),
            .req_id = hx.req_id,
        });
        metrics.incSignupFailed(.db_error);
        common.internalOperationError(hx.res, "Failed to finish setting up the new account", hx.req_id);
        return;
    };
    defer bootstrap.deinit(hx.alloc);

    // Write tenant_id + default role back to Clerk
    // publicMetadata so the user's next session JWT carries both. The DB
    // row is already provisioned — the writeback is best-effort; an
    // upstream outage logs + metrics-increments but does not fail signup.
    writePublicMetadata(hx, oidc_subject, bootstrap.tenant_id);

    captureSignupEvent(hx, oidc_subject, email, bootstrap);
    hx.ok(.ok, .{
        .workspace_id = bootstrap.workspace_id,
        .workspace_name = bootstrap.workspace_name,
        .created = bootstrap.created,
    });
}

fn writePublicMetadata(hx: Hx, oidc_subject: []const u8, tenant_id: []const u8) void {
    clerk_backend.patchUserPublicMetadata(hx.ctx.clerk_secret_key, hx.alloc, oidc_subject, tenant_id, DEFAULT_SIGNUP_SCOPES) catch |err| {
        log.warn(
            "metadata_writeback_failed",
            .{
                .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
                .err = @errorName(err),
                .oidc = oidc_subject,
                .tenant = tenant_id,
                .req_id = hx.req_id,
            },
        );
        metrics.incSignupFailed(.metadata_writeback);
        // Do not fail the webhook — the DB row exists; the user's first
        // API call will 401 (workspaces/lifecycle raises `ERR_UNAUTHORIZED`
        // on a null tenant_id) until their Clerk session refreshes with
        // the written-back claims. Operators can repair via the Clerk
        // Dashboard.
    };
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

fn runDelete(hx: Hx, oidc_subject: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        log.warn("delete_pool_acquire_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .oidc = oidc_subject, .req_id = hx.req_id });
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const purged = account_teardown.purgeByOidcSubject(conn, hx.alloc, oidc_subject) catch |err| {
        log.warn("delete_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .oidc = oidc_subject, .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    log.debug("user_deleted", .{ .oidc = oidc_subject, .purged = purged, .req_id = hx.req_id });
    hx.ok(.ok, .{ .deleted = true });
}
