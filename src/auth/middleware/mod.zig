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

pub const chain = @import("chain.zig");
pub const auth_ctx = @import("auth_ctx.zig");
pub const errors = @import("errors.zig");

pub const Middleware = chain.Middleware;
pub const Outcome = chain.Outcome;
pub const run = chain.run;

pub const AuthCtx = auth_ctx.AuthCtx;
pub const WriteErrorFn = auth_ctx.WriteErrorFn;

pub const admin_api_key = @import("admin_api_key.zig");
pub const bearer_oidc = @import("bearer_oidc.zig");
pub const bearer_or_api_key = @import("bearer_or_api_key.zig");
pub const tenant_api_key = @import("tenant_api_key.zig");
pub const require_role = @import("require_role.zig");
pub const webhook_hmac = @import("webhook_hmac.zig");
pub const webhook_url_secret = @import("webhook_url_secret.zig");
pub const webhook_sig_mod = @import("webhook_sig.zig");
pub const slack_signature = @import("slack_signature.zig");
pub const svix_signature_mod = @import("svix_signature.zig");
pub const oauth_state = @import("oauth_state.zig");

pub const AdminApiKey = admin_api_key.AdminApiKey;
pub const BearerOidc = bearer_oidc.BearerOidc;
pub const BearerOrApiKey = bearer_or_api_key.BearerOrApiKey;
pub const TenantApiKey = tenant_api_key.TenantApiKey;
pub const RequireRole = require_role.RequireRole;
pub const WebhookHmac = webhook_hmac.WebhookHmac;
pub const WebhookUrlSecret = webhook_url_secret.WebhookUrlSecret;
pub const SlackSignature = slack_signature.SlackSignature;
pub const SvixSignature = svix_signature_mod.SvixSignature;
pub const OAuthState = oauth_state.OAuthState;

pub const AuthPrincipal = auth_ctx.AuthPrincipal;

/// Boot-time registry of pre-instantiated middleware structs and pre-built
/// policy chains.
///
/// LIFETIME RULE: call `initChains()` exactly once, AFTER the registry is
/// in its final memory location (i.e. after all struct fields are set and
/// before the server starts accepting requests). Do NOT move or copy the
/// registry struct after `initChains()` — the chain arrays store pointers
/// into fields of this struct.
pub const MiddlewareRegistry = struct {
    // ── Concrete middleware instances ─────────────────────────────────────
    bearer_or_api_key: BearerOrApiKey,
    admin_api_key_mw: AdminApiKey,
    tenant_api_key_mw: TenantApiKey,
    require_role_admin: RequireRole,
    require_role_operator: RequireRole,
    slack_sig: SlackSignature,
    webhook_hmac_mw: WebhookHmac,
    oauth_state_mw: OAuthState,
    webhook_url_secret_mw: WebhookUrlSecret,

    // ── Pre-built policy chains ────────────────────────────────────────────
    // Populated by initChains(). Fixed-size arrays so policy methods can
    // return slices without allocating per request.
    _bearer_chain: [1]Middleware(AuthCtx) = undefined,
    _admin_chain: [2]Middleware(AuthCtx) = undefined,
    _operator_chain: [2]Middleware(AuthCtx) = undefined,
    _admin_api_key_chain: [1]Middleware(AuthCtx) = undefined,
    _webhook_hmac_chain: [1]Middleware(AuthCtx) = undefined,
    _webhook_secret_chain: [1]Middleware(AuthCtx) = undefined,
    _slack_chain: [1]Middleware(AuthCtx) = undefined,
    _oauth_chain: [1]Middleware(AuthCtx) = undefined,
    // M28_001: webhook_sig is generic over LookupCtx, so the host
    // calls .middleware() on the concrete instance and passes the
    // pre-built Middleware(AuthCtx) value here. No *anyopaque needed.
    _webhook_sig_chain: [1]Middleware(AuthCtx) = undefined,
    // M28_001 §5: svix middleware is also generic over LookupCtx; host
    // supplies the built Middleware(AuthCtx) after construction.
    _svix_chain: [1]Middleware(AuthCtx) = undefined,

    /// Build the policy chain arrays. Must be called once after the registry
    /// struct is placed in its final memory location.
    pub fn initChains(self: *MiddlewareRegistry) void {
        // Wire the tenant-key pointer into bearer_or_api_key so `zmb_t_`-
        // prefixed tokens delegate to the DB-backed lookup path.
        self.bearer_or_api_key.tenant_api_key = &self.tenant_api_key_mw;
        self._bearer_chain = .{self.bearer_or_api_key.middleware()};
        self._admin_chain = .{
            self.bearer_or_api_key.middleware(),
            self.require_role_admin.middleware(),
        };
        self._operator_chain = .{
            self.bearer_or_api_key.middleware(),
            self.require_role_operator.middleware(),
        };
        self._admin_api_key_chain = .{self.admin_api_key_mw.middleware()};
        self._webhook_hmac_chain = .{self.webhook_hmac_mw.middleware()};
        self._webhook_secret_chain = .{self.webhook_url_secret_mw.middleware()};
        self._slack_chain = .{self.slack_sig.middleware()};
        self._oauth_chain = .{self.oauth_state_mw.middleware()};
        // _webhook_sig_chain is set by the host via setWebhookSig()
    }

    /// Host sets the webhook sig middleware after constructing the
    /// generic WebhookSig(LookupCtx) instance with the concrete type.
    pub fn setWebhookSig(self: *MiddlewareRegistry, mw: Middleware(AuthCtx)) void {
        self._webhook_sig_chain = .{mw};
    }

    /// Host sets the Svix middleware after constructing the generic
    /// SvixSignature(LookupCtx) instance with the concrete type.
    pub fn setSvixSig(self: *MiddlewareRegistry, mw: Middleware(AuthCtx)) void {
        self._svix_chain = .{mw};
    }

    // ── Policy accessors ────────────────────────────────────────────────────

    /// No authentication required.
    pub const none: []const Middleware(AuthCtx) = &.{};

    /// Bearer token or admin API key, any role.
    pub fn bearer(self: *MiddlewareRegistry) []const Middleware(AuthCtx) {
        return &self._bearer_chain;
    }

    /// Bearer token or admin API key, admin role required.
    pub fn admin(self: *MiddlewareRegistry) []const Middleware(AuthCtx) {
        return &self._admin_chain;
    }

    /// Bearer token or admin API key, operator role required.
    pub fn operator(self: *MiddlewareRegistry) []const Middleware(AuthCtx) {
        return &self._operator_chain;
    }

    /// Admin API key only (no JWT path).
    pub fn adminApiKey(self: *MiddlewareRegistry) []const Middleware(AuthCtx) {
        return &self._admin_api_key_chain;
    }

    /// HMAC-SHA256 body signature (approval/generic webhooks).
    pub fn webhookHmac(self: *MiddlewareRegistry) []const Middleware(AuthCtx) {
        return &self._webhook_hmac_chain;
    }

    /// URL-embedded per-zombie secret.
    pub fn webhookSecret(self: *MiddlewareRegistry) []const Middleware(AuthCtx) {
        return &self._webhook_secret_chain;
    }

    /// Slack x-slack-signature verification.
    pub fn slack(self: *MiddlewareRegistry) []const Middleware(AuthCtx) {
        return &self._slack_chain;
    }

    /// OAuth state nonce + HMAC (GitHub/Slack OAuth callbacks).
    pub fn oauthCallback(self: *MiddlewareRegistry) []const Middleware(AuthCtx) {
        return &self._oauth_chain;
    }

    /// Unified webhook auth: URL secret + Bearer token (M28_001).
    pub fn webhookSig(self: *MiddlewareRegistry) []const Middleware(AuthCtx) {
        return &self._webhook_sig_chain;
    }

    /// Svix v1 multi-sig HMAC (Clerk) — M28_001 §5.
    pub fn svix(self: *MiddlewareRegistry) []const Middleware(AuthCtx) {
        return &self._svix_chain;
    }
};
