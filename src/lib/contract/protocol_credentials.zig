//! On-demand credential-mint wire sub-protocol (M102 §3) — the request/response
//! the runner forwards for `POST /v1/runners/me/credentials/mint`. Split out of
//! `protocol.zig` (RULE FLL) and re-exported there, so `protocol.MintCredentialRequest`
//! (and its sibling) are unchanged for callers. Mirrors the `protocol_memory.zig`
//! split. The route path itself stays in `protocol.zig` beside the other PATH_ consts.

/// POST /v1/runners/me/credentials/mint request body. The runner forwards the
/// sandboxed child's ask verbatim. `lease_id` binds the mint to the lease's
/// workspace server-side — the child never names a workspace, so a prompt-injected
/// child cannot mint for another tenant (Invariant 2). `integration` selects the
/// connected integration (`"github"`); `scope` is an optional integration-specific
/// narrowing the broker may honour. Not secrets — the request carries no token.
pub const MintCredentialRequest = struct {
    lease_id: []const u8,
    integration: []const u8,
    scope: ?[]const u8 = null,
};

/// POST /v1/runners/me/credentials/mint reply (200). `token` is the short-lived,
/// workspace-scoped credential the tool boundary substitutes for
/// `${secrets.<integration>.token}`; it is secret (VLT) — never logged, never
/// echoed into a frame. `expires_at_ms` is an epoch-ms bound the runner/child use
/// to re-mint before expiry. A non-200 carries the typed error envelope
/// (`UZ-CRED-*` unknown-integration, `UZ-GH-*` reconnect/mint-failed).
pub const MintCredentialResponse = struct {
    token: []const u8,
    expires_at_ms: i64,
};
