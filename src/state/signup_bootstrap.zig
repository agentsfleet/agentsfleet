//! M11_003 signup bootstrap facade.
//!
//! Given an OIDC subject + email from the Clerk `user.created` webhook,
//! atomically provisions a personal account in a single SQL transaction:
//!
//!   1. tenant                                — one per user
//!   2. user (oidc_subject unique)            — Clerk identity hook
//!   3. membership (role=owner)               — links user to tenant
//!   4. workspace (Heroku-style unique name)  — default workspace
//!   5. workspace_credit_state (0 cents)      — starter balance
//!   6. workspace_credit_audit (signup row)   — provenance trail
//!
//! Any failure rolls back. Idempotent on `oidc_subject` — replayed webhooks
//! return existing rows without re-inserting. Platform LLM keys + provider
//! attachment are deliberately out of scope; resolver reads
//! core.platform_llm_keys at LLM-call time (runtime-only).

const std = @import("std");
const pg = @import("pg");
const id_format = @import("../types/id_format.zig");
const credit_store = @import("workspace_credit_store.zig");
const heroku_names = @import("heroku_names.zig");
const store = @import("signup_bootstrap_store.zig");

const log = std.log.scoped(.state);

/// Per-tenant uniqueness + a freshly created tenant makes collisions
/// practically impossible; cap is a guard against a buggy generator.
pub const MAX_NAME_ATTEMPTS: u8 = 8;

/// Stamped into workspace_credit_audit.actor on the zero-credit row so
/// analytics can identify signup-bootstrapped workspaces.
pub const BOOTSTRAP_ACTOR = "signup_bootstrap";

/// Tenant-level role for the signup user. Personal accounts have exactly
/// one owner; team accounts are a future milestone.
pub const OWNER_ROLE = "owner";

pub const BootstrapError = error{
    /// Ran out of `MAX_NAME_ATTEMPTS` without landing a unique name. Only
    /// possible with a broken generator or exotic tenant state.
    WorkspaceNameCollisionExhausted,
};

pub const BootstrapParams = struct {
    oidc_subject: []const u8,
    email: []const u8,
    display_name: ?[]const u8 = null,
};

pub const Bootstrap = struct {
    user_id: []u8,
    tenant_id: []u8,
    workspace_id: []u8,
    workspace_name: []u8,
    /// `true` on fresh bootstrap, `false` on idempotent replay.
    created: bool,

    pub fn deinit(self: *Bootstrap, alloc: std.mem.Allocator) void {
        alloc.free(self.user_id);
        alloc.free(self.tenant_id);
        alloc.free(self.workspace_id);
        alloc.free(self.workspace_name);
    }
};

/// Injectable name generator used by `pickUniqueWorkspaceName`. Production
/// passes `defaultHerokuNameGen`; tests inject a sequence to exercise retry.
pub const NameGenFn = *const fn (std.mem.Allocator) anyerror![]u8;

pub fn defaultHerokuNameGen(alloc: std.mem.Allocator) anyerror![]u8 {
    return heroku_names.generate(alloc);
}

pub fn bootstrapPersonalAccount(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    params: BootstrapParams,
) !Bootstrap {
    if (try store.findExistingByOidcSubject(conn, alloc, params.oidc_subject)) |existing| {
        log.info("signup.replay oidc_subject={s} workspace={s}", .{ params.oidc_subject, existing.workspace_id });
        return .{
            .user_id = existing.user_id,
            .tenant_id = existing.tenant_id,
            .workspace_id = existing.workspace_id,
            .workspace_name = existing.workspace_name,
            .created = false,
        };
    }

    return bootstrapTransaction(conn, alloc, params, defaultHerokuNameGen);
}

/// Transactional core split from the entry point so tests can inject a
/// deterministic name generator.
pub fn bootstrapTransaction(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    params: BootstrapParams,
    gen: NameGenFn,
) !Bootstrap {
    _ = try conn.exec("BEGIN", .{});
    var tx_open = true;
    errdefer if (tx_open) {
        _ = conn.exec("ROLLBACK", .{}) catch {};
    };

    const now_ms = std.time.milliTimestamp();

    const tenant_id = try id_format.allocUuidV7(alloc);
    errdefer alloc.free(tenant_id);
    const tenant_name = try derivePersonalTenantName(alloc, params.email);
    defer alloc.free(tenant_name);
    try store.insertTenant(conn, .{
        .tenant_id = tenant_id,
        .name = tenant_name,
        .now_ms = now_ms,
    });

    const user_id = try id_format.allocUuidV7(alloc);
    errdefer alloc.free(user_id);
    try store.insertUser(conn, .{
        .user_id = user_id,
        .tenant_id = tenant_id,
        .oidc_subject = params.oidc_subject,
        .email = params.email,
        .display_name = params.display_name,
        .now_ms = now_ms,
    });

    try store.insertMembership(conn, tenant_id, user_id, OWNER_ROLE, now_ms);

    const workspace_id = try id_format.allocUuidV7(alloc);
    errdefer alloc.free(workspace_id);
    const workspace_name = try pickUniqueWorkspaceName(
        conn,
        alloc,
        tenant_id,
        workspace_id,
        BOOTSTRAP_ACTOR,
        now_ms,
        gen,
    );
    errdefer alloc.free(workspace_name);

    try initializeZeroCredit(conn, alloc, workspace_id, now_ms);

    _ = try conn.exec("COMMIT", .{});
    tx_open = false;

    log.info("signup.bootstrapped user={s} tenant={s} workspace={s} workspace_name={s}", .{ user_id, tenant_id, workspace_id, workspace_name });

    return .{
        .user_id = @constCast(user_id),
        .tenant_id = @constCast(tenant_id),
        .workspace_id = @constCast(workspace_id),
        .workspace_name = workspace_name,
        .created = true,
    };
}

/// Retry up to `MAX_NAME_ATTEMPTS` to land a `(tenant_id, name)` that
/// satisfies `uq_workspaces_tenant_name`. On return, the workspace row has
/// been INSERTed inside the enclosing transaction and the name slice is
/// owned by the caller's allocator.
pub fn pickUniqueWorkspaceName(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    tenant_id: []const u8,
    workspace_id: []const u8,
    created_by: []const u8,
    now_ms: i64,
    gen: NameGenFn,
) ![]u8 {
    var attempt: u8 = 0;
    while (attempt < MAX_NAME_ATTEMPTS) : (attempt += 1) {
        const candidate = try gen(alloc);
        errdefer alloc.free(candidate);

        const inserted = try store.tryInsertWorkspace(conn, .{
            .workspace_id = workspace_id,
            .tenant_id = tenant_id,
            .name = candidate,
            .created_by = created_by,
            .now_ms = now_ms,
        });
        if (inserted) return candidate;

        alloc.free(candidate);
        log.warn("signup.name_collision tenant={s} attempt={d}", .{ tenant_id, attempt + 1 });
    }
    return BootstrapError.WorkspaceNameCollisionExhausted;
}

fn initializeZeroCredit(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    now_ms: i64,
) !void {
    try credit_store.upsertCreditState(conn, alloc, workspace_id, .{
        .currency = "USD",
        .initial_credit_cents = 0,
        .consumed_credit_cents = 0,
        .remaining_credit_cents = 0,
        .exhausted_at = null,
    }, now_ms);
    try credit_store.insertAudit(
        conn,
        alloc,
        workspace_id,
        "CREDIT_GRANTED",
        0,
        0,
        "signup_bootstrap",
        BOOTSTRAP_ACTOR,
        "{}",
    );
}

fn derivePersonalTenantName(alloc: std.mem.Allocator, email: []const u8) ![]u8 {
    const at = std.mem.indexOfScalar(u8, email, '@') orelse email.len;
    const local = email[0..at];
    if (local.len == 0) return alloc.dupe(u8, "personal");
    return alloc.dupe(u8, local);
}
