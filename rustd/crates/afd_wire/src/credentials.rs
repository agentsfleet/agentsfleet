//! On-demand credential minting at the tool boundary.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// `POST /v1/runners/me/credentials/mint` request.
///
/// The runner forwards the sandboxed child's ask verbatim. `lease_id` binds the
/// mint to the lease's workspace SERVER-SIDE — the child never names a
/// workspace, so a prompt-injected child cannot mint for another tenant. The
/// request carries no token.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MintCredentialRequest<'a> {
    /// The lease the mint is bound to.
    #[serde(borrow)]
    pub lease_id: Cow<'a, str>,
    /// Which connected integration to mint for.
    #[serde(borrow)]
    pub integration: Cow<'a, str>,
    /// An integration-specific narrowing the broker may honour.
    #[serde(borrow)]
    pub scope: Option<Cow<'a, str>>,
}

/// `POST /v1/runners/me/credentials/mint` reply.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MintCredentialResponse<'a> {
    /// The short-lived, workspace-scoped credential. Secret — never logged,
    /// never echoed into a frame.
    #[serde(borrow)]
    pub token: Cow<'a, str>,
    /// Epoch milliseconds after which it stops working, so the caller re-mints
    /// before expiry rather than on failure.
    pub expires_at_ms: i64,
}
