//! Command-line credential endpoints — `POST` and `GET /v1/cli-credentials`,
//! `DELETE /v1/cli-credentials/{id}`.
//!
//! These three close the credential's life. `agentsfleet login` spends its
//! recovered session token on the mint and persists what comes back, so the
//! durable credential rather than a token that dies in a minute is what reaches
//! disk. The list is how an operator sees which terminals hold one — and
//! therefore how a shared credential becomes visible, since a sharer mints on
//! their own machine and arrives as a second live row under one user carrying a
//! different machine name. The revoke is what makes `logout` mean something on
//! the server rather than only in the local state directory.
//!
//! **Who may call them, and why scope cannot be the answer.** A credential
//! names a person, so these routes admit only the principal classes that ARE a
//! person: the browser-issued session token (which is how login's exchange
//! authorises itself, inside its own short window) and an existing command-line
//! credential (which is how a logged-in terminal manages its own). Minting is
//! narrower still and takes the session token alone, so that a credential
//! cannot mint its own successor — see `requireFreshSessionSubject`. A tenant
//! `agt_t` key is refused here, and a required scope could not express that —
//! a tenant key carries the whole tenant grant, so it already holds every scope
//! these routes could ask for. The refusal is on principal MODE because that is
//! the only thing that distinguishes an organisation from a human.
//!
//! Nothing here reads or returns credential material beyond the single mint
//! response, which goes out through the erasing writer: the raw value exists
//! once, in that body, and is unrecoverable afterwards.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");

const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const sql = @import("../../../state/sql.zig");
const store = @import("../../../state/cli_credentials.zig");
const cli_credential = @import("../../../auth/cli_credential.zig");
const session_helpers = @import("session_helpers.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;

const Hx = hx_mod.Hx;
const log = logging.scoped(.cli_credentials);

const S_ID_MUST_BE_A_VALID_UUIDV7 = "id must be a valid UUIDv7";
const S_CREDENTIAL_NOT_FOUND = "Command-line credential not found";
const S_BODY_REQUIRED = "Request body required";
const S_MALFORMED_JSON = "Malformed JSON body";
const S_MACHINE_NAME_INVALID =
    "machine_name must be 1-64 chars: letters, digits, hyphen, underscore, dot";
const S_PERSON_REQUIRED =
    "A command-line credential belongs to a person; a tenant API key cannot manage one";
const S_UNKNOWN_SUBJECT = "Authenticated subject has no user record";
const S_SESSION_REQUIRED =
    "Minting a command-line credential requires a browser sign-in; an existing credential cannot mint another";

const EV_MINTED = "credential_minted";
const EV_REVOKED = "credential_revoked";

/// The user row these endpoints write against. `core.cli_credentials.user_id`
/// is a foreign key to `core.users(id)`, while a principal carries the identity
/// provider's subject — so every call resolves one before it can write.
const UserIdentity = struct {
    id: []const u8,
    tenant_id: []const u8,
};

/// The authenticated subject, or null after the refusal is already written.
///
/// `.api_key` is refused rather than falling through: a tenant key sets a
/// non-null `user_id` (its free-text `created_by`), so a null check alone would
/// admit it and quietly let an organisation mint a credential in a person's
/// name — the exact widening Invariant 1 forbids.
fn requirePersonSubject(hx: Hx) ?[]const u8 {
    switch (hx.principal.mode) {
        .jwt_oidc, .cli_credential => {},
        else => {
            hx.fail(ec.ERR_FORBIDDEN, S_PERSON_REQUIRED);
            return null;
        },
    }
    const subject = hx.principal.user_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, S_PERSON_REQUIRED);
        return null;
    };
    if (subject.len == 0) {
        hx.fail(ec.ERR_FORBIDDEN, S_PERSON_REQUIRED);
        return null;
    }
    return subject;
}

/// Minting additionally refuses `.cli_credential`, though listing and revoking
/// accept it.
///
/// A credential that can mint another credential is a self-renewing one: each
/// mints the next under a machine name of the caller's choosing, revoking any
/// single row leaves its siblings live, and the person holding the account
/// cannot tell how many exist. That turns one compromised login — a session
/// token good for about a minute — into permanent access that outlives every
/// remedy short of deleting the user. Minting therefore costs a browser
/// session every time, which is the one step a stolen credential cannot
/// replay. Listing and revoking stay open to a credential because a terminal
/// must be able to see and end its own access without a browser.
fn requireFreshSessionSubject(hx: Hx) ?[]const u8 {
    if (hx.principal.mode != .jwt_oidc) {
        hx.fail(ec.ERR_FORBIDDEN, S_SESSION_REQUIRED);
        return null;
    }
    return requirePersonSubject(hx);
}

/// Resolve the subject to its user row. Null after the refusal is written.
/// Both slices are owned by `hx.alloc`.
///
/// Takes `*pg.Conn` concretely rather than `anytype`: a generic parameter is
/// only analysed once something instantiates it, which is how this workstream's
/// own store reached review with column-shape mistakes behind a green build.
fn resolveUser(hx: Hx, conn: *pg.Conn, subject: []const u8) ?UserIdentity {
    var q = PgQuery.from(conn.query(sql.SELECT_USER_IDENTITY_BY_SUBJECT, .{subject}) catch {
        hx.fail(ec.ERR_INTERNAL_OPERATION_FAILED, S_UNKNOWN_SUBJECT);
        return null;
    });
    defer q.deinit();

    const row = (q.next() catch {
        hx.fail(ec.ERR_INTERNAL_OPERATION_FAILED, S_UNKNOWN_SUBJECT);
        return null;
    }) orelse {
        // A live token for a subject with no local row: the account was torn
        // down, or the two stores diverged. Refused rather than provisioned on
        // the fly, because minting a user row from an authenticate path is how
        // an identity ends up existing in two places with different truths.
        hx.fail(ec.ERR_FORBIDDEN, S_UNKNOWN_SUBJECT);
        return null;
    };

    const id_raw = row.get([]const u8, 0) catch {
        hx.fail(ec.ERR_INTERNAL_OPERATION_FAILED, S_UNKNOWN_SUBJECT);
        return null;
    };
    const tenant_raw = row.get([]const u8, 1) catch {
        hx.fail(ec.ERR_INTERNAL_OPERATION_FAILED, S_UNKNOWN_SUBJECT);
        return null;
    };
    const id = hx.alloc.dupe(u8, id_raw) catch {
        hx.fail(ec.ERR_INTERNAL_OPERATION_FAILED, S_UNKNOWN_SUBJECT);
        return null;
    };
    const tenant_id = hx.alloc.dupe(u8, tenant_raw) catch {
        hx.alloc.free(id);
        hx.fail(ec.ERR_INTERNAL_OPERATION_FAILED, S_UNKNOWN_SUBJECT);
        return null;
    };
    return .{ .id = id, .tenant_id = tenant_id };
}

const MintBody = struct {
    machine_name: []const u8,
};

/// `POST /v1/cli-credentials` — mint this machine's credential, revoking
/// whatever the same machine left behind. The raw value is returned once.
pub fn innerMintCliCredential(hx: Hx, req: *httpz.Request) void {
    const subject = requireFreshSessionSubject(hx) orelse return;

    const raw_body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, S_BODY_REQUIRED);
        return;
    };
    const parsed = std.json.parseFromSlice(MintBody, hx.alloc, raw_body, .{
        .ignore_unknown_fields = true,
    }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();

    if (!cli_credential.isValidMachineName(parsed.value.machine_name)) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MACHINE_NAME_INVALID);
        return;
    }

    // SAFETY: buildScratch writes every field before any read below.
    var scratch: session_helpers.RequestScratch = undefined;
    session_helpers.buildScratch(&scratch, req);

    var db = hx.db() catch return;
    defer db.end();

    const user = resolveUser(hx, db.conn, subject) orelse return;
    defer hx.alloc.free(user.id);
    defer hx.alloc.free(user.tenant_id);

    // The deployment is the one answering this request, never a value the
    // caller supplied — a credential and the deployment that minted it are one
    // fact, and a client-asserted host would let them disagree.
    const minted = store.mint(hx.alloc, db.conn, .{
        .user_id = user.id,
        .tenant_id = user.tenant_id,
        .machine_name = parsed.value.machine_name,
        .deployment = hx.ctx.api_url,
        .created_from_address = scratch.derived.ip,
    }) catch {
        hx.fail(ec.ERR_INTERNAL_OPERATION_FAILED, "Could not mint a command-line credential");
        return;
    };
    defer minted.deinit(hx.alloc);

    // Attribution is a mint-time fact: recorded once, here, and never written
    // again on the authenticate path. The credential itself is absent from
    // this line and from every other emitted surface.
    log.info(EV_MINTED, .{
        .credential_id = minted.id,
        .machine_name = parsed.value.machine_name,
        .deployment = hx.ctx.api_url,
    });

    // The erasing writer: this body is the only place the raw value exists
    // outside the caller's process, and its serialized bytes are wiped after
    // the write rather than lingering in a reusable response buffer.
    hx.okSensitive(.created, .{
        .id = minted.id,
        .credential = minted.secret,
        .machine_name = parsed.value.machine_name,
        .deployment = hx.ctx.api_url,
    });
}

/// `GET /v1/cli-credentials` — this user's live credentials. Never returns
/// anything that authenticates; `prefix` is a display fragment only.
pub fn innerListCliCredentials(hx: Hx) void {
    const subject = requirePersonSubject(hx) orelse return;

    var db = hx.db() catch return;
    defer db.end();

    const user = resolveUser(hx, db.conn, subject) orelse return;
    defer hx.alloc.free(user.id);
    defer hx.alloc.free(user.tenant_id);

    const rows = store.listForUser(hx.alloc, db.conn, user.id) catch {
        hx.fail(ec.ERR_INTERNAL_OPERATION_FAILED, "Could not list command-line credentials");
        return;
    };
    defer store.deinitList(hx.alloc, rows);

    hx.ok(.ok, .{ .credentials = rows });
}

/// `DELETE /v1/cli-credentials/{id}` — revoke one of this user's credentials.
/// Scoped to the owner in the statement itself, so a guessed identifier
/// belonging to somebody else revokes nothing and reads as not found.
pub fn innerRevokeCliCredential(hx: Hx, credential_id: []const u8) void {
    const subject = requirePersonSubject(hx) orelse return;

    if (!id_format.isUuidV7(credential_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_ID_MUST_BE_A_VALID_UUIDV7);
        return;
    }

    var db = hx.db() catch return;
    defer db.end();

    const user = resolveUser(hx, db.conn, subject) orelse return;
    defer hx.alloc.free(user.id);
    defer hx.alloc.free(user.tenant_id);

    const revoked = store.revokeById(db.conn, credential_id, user.id) catch {
        hx.fail(ec.ERR_INTERNAL_OPERATION_FAILED, "Could not revoke the command-line credential");
        return;
    };
    if (!revoked) {
        // Already revoked, or never this user's. One answer for both: telling
        // them apart would confirm the existence of another person's
        // credential to whoever guessed its identifier.
        hx.fail(ec.ERR_CLI_CREDENTIAL_NOT_FOUND, S_CREDENTIAL_NOT_FOUND);
        return;
    }

    log.info(EV_REVOKED, .{ .credential_id = credential_id });
    hx.noContent();
}

test {
    std.testing.refAllDecls(@This());
    _ = @import("cli_credentials_test.zig");
}
