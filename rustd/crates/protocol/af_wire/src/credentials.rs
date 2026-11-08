//! On-demand credential minting at the tool boundary.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// Requests a tool credential for the current work assignment. `lease_id` determines
/// workspace access; the body cannot select another workspace and contains no token.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MintCredentialRequest<'a> {
    /// Current work assignment authorizing this credential request.
    #[serde(borrow)]
    pub lease_id: Cow<'a, str>,
    /// Connected integration that needs a credential.
    #[serde(borrow)]
    pub integration: Cow<'a, str>,
    /// An integration-specific narrowing the broker may honour.
    #[serde(borrow)]
    pub scope: Option<Cow<'a, str>>,
}

/// Newly issued tool credential.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MintCredentialResponse<'a> {
    /// The short-lived, workspace-scoped credential. Secret — never logged,
    /// never echoed into a frame.
    #[serde(borrow)]
    pub token: Cow<'a, str>,
    /// When it stops working, so the caller re-mints before expiry rather than on failure.
    ///
    /// A credential with no upstream expiry carries the far-future instant
    /// `9999-12-30T22:00:00.999Z`, so the caller's one comparison covers it.
    #[serde(with = "crate::instant")]
    pub expires_at: jiff::Timestamp,
}
