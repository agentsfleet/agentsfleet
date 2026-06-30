//! Public surface of the auth middleware layer (M18_002).
//!
//! Re-exports the chain runner and the concrete middleware implementations.
//! Also defines `MiddlewareRegistry` — the boot-time struct that holds
//! pre-instantiated middleware and pre-built policy chains. The host
//! (src/cmd/serve.zig) constructs one registry at server boot and passes
//! a pointer into the HTTP dispatcher via `App.registry`.
//!
//! C.2 NOTE: `MiddlewareRegistry` is intentionally kept in this auth-layer
//! module (not in src/http/handlers/common.zig) so the portability gate
//! (`make test-auth`) covers the complete registry without http-layer
//! imports. RULE FLL prevents touching common.zig (782 lines) without a
//! split; the registry therefore lives here and the dispatcher receives it
//! via App, not via Context.

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");

pub const Middleware = chain.Middleware;
pub const run = chain.run;

pub const AuthCtx = auth_ctx.AuthCtx;

const bearer_or_api_key = @import("bearer_or_api_key.zig");
pub const tenant_api_key = @import("tenant_api_key.zig");
pub const runner_bearer = @import("runner_bearer.zig");
const require_scope = @import("require_scope.zig");
const webhook_hmac = @import("webhook_hmac.zig");
pub const webhook_sig_mod = @import("webhook_sig.zig");
pub const svix_signature_mod = @import("svix_signature.zig");

const BearerOrApiKey = bearer_or_api_key.BearerOrApiKey;
const TenantApiKey = tenant_api_key.TenantApiKey;
const RunnerBearer = runner_bearer.RunnerBearer;
const RequireScope = require_scope.RequireScope;
const WebhookHmac = webhook_hmac.WebhookHmac;

/// Boot-time registry of pre-instantiated middleware structs and pre-built
/// policy chains.
///
/// LIFETIME RULE: call `initChains()` exactly once, AFTER the registry is
/// in its final memory location (i.e. after all struct fields are set and
/// before the server starts accepting requests). Do NOT move or copy the
/// registry struct after `initChains()` — the chain arrays store pointers
/// into fields of this struct.
pub const MiddlewareRegistry = struct {
    const Self = @This();

    // ── Concrete middleware instances ─────────────────────────────────────
    bearer_or_api_key: BearerOrApiKey,
    tenant_api_key_mw: TenantApiKey,
    runner_bearer_mw: RunnerBearer,
    // One capability gate for every authenticated route. The
    // route's required scopes ride `AuthCtx.required_scopes`, set per-request by
    // the host; this single middleware enforces them. Replaces the per-role
    // RequireRole instances and the PlatformAdmin gate.
    require_scope_mw: RequireScope,
    webhook_hmac_mw: WebhookHmac,

    // ── Pre-built policy chains ────────────────────────────────────────────
    // Populated by initChains(). Fixed-size arrays so policy methods can
    // return slices without allocating per request.
    // Authenticated chain = identity (bearer/api-key) + capability (require_scope).
    // SAFETY: populated by initChains() before any policy method reads it.
    _bearer_chain: [2]Middleware(AuthCtx) = undefined,
    // Runner chain = runner-token identity + capability (require_scope → runner:self).
    // SAFETY: populated by initChains() before any policy method reads it.
    _runner_chain: [2]Middleware(AuthCtx) = undefined,
    // SAFETY: populated by initChains() before any policy method reads it.
    _webhook_hmac_chain: [1]Middleware(AuthCtx) = undefined,
    // webhook_sig is generic over LookupCtx, so the host calls
    // .middleware() on the concrete instance and passes the pre-built
    // Middleware(AuthCtx) value here. No *anyopaque needed.
    // SAFETY: host assigns the concrete Middleware(AuthCtx) before consumer reads.
    _webhook_sig_chain: [1]Middleware(AuthCtx) = undefined,
    // svix middleware is also generic over LookupCtx; host supplies the
    // built Middleware(AuthCtx) after construction.
    // SAFETY: host assigns the concrete Middleware(AuthCtx) before consumer reads.
    _svix_chain: [1]Middleware(AuthCtx) = undefined,

    /// Build the policy chain arrays. Must be called once after the registry
    /// struct is placed in its final memory location.
    pub fn initChains(self: *Self) void {
        // Wire the tenant-key pointer into bearer_or_api_key so `agt_t`-
        // prefixed tokens delegate to the DB-backed lookup path.
        self.bearer_or_api_key.tenant_api_key = &self.tenant_api_key_mw;
        self._bearer_chain = .{
            self.bearer_or_api_key.middleware(),
            self.require_scope_mw.middleware(),
        };
        self._runner_chain = .{
            self.runner_bearer_mw.middleware(),
            self.require_scope_mw.middleware(),
        };
        self._webhook_hmac_chain = .{self.webhook_hmac_mw.middleware()};
        // _webhook_sig_chain is set by the host via setWebhookSig()
    }

    /// Host sets the webhook sig middleware after constructing the
    /// generic WebhookSig(LookupCtx) instance with the concrete type.
    pub fn setWebhookSig(self: *Self, mw: Middleware(AuthCtx)) void {
        self._webhook_sig_chain = .{mw};
    }

    /// Host sets the Svix middleware after constructing the generic
    /// SvixSignature(LookupCtx) instance with the concrete type.
    pub fn setSvixSig(self: *Self, mw: Middleware(AuthCtx)) void {
        self._svix_chain = .{mw};
    }

    // ── Policy accessors ────────────────────────────────────────────────────

    /// No authentication required.
    pub const none: []const Middleware(AuthCtx) = &.{};

    /// Bearer token or `agt_t` API key, then the capability gate. The route's
    /// required scopes (resolved from `route_scopes` onto `AuthCtx.required_scopes`)
    /// decide what the principal must hold; an empty requirement is
    /// authenticated-only. This single policy replaces the former
    /// bearer/admin/operator/platformAdmin chains — capability is now data
    /// (the route→scope table), not a per-policy middleware.
    pub fn bearer(self: *Self) []const Middleware(AuthCtx) {
        return &self._bearer_chain;
    }

    /// Runner-token (`agt_r`) machine principal + capability gate (the
    /// `/v1/runners/me/*` routes require `runner:self`, which only this
    /// principal carries). No JWKS/tenant fall-through.
    pub fn runnerBearer(self: *Self) []const Middleware(AuthCtx) {
        return &self._runner_chain;
    }

    /// HMAC-SHA256 body signature (approval/generic webhooks).
    pub fn webhookHmac(self: *Self) []const Middleware(AuthCtx) {
        return &self._webhook_hmac_chain;
    }

    /// Per-agent HMAC signature for webhooks routed to a agent (HMAC-only —
    /// no Bearer fallback). The lookup function returns the HMAC scheme +
    /// secret resolved from the workspace credential identified by the
    /// matching `triggers[].source` entry (or an explicit
    /// `triggers[].credential_name` override).
    pub fn webhookSig(self: *Self) []const Middleware(AuthCtx) {
        return &self._webhook_sig_chain;
    }

    /// Svix v1 multi-sig HMAC (Clerk) — M28_001 §5.
    pub fn svix(self: *Self) []const Middleware(AuthCtx) {
        return &self._svix_chain;
    }
};
