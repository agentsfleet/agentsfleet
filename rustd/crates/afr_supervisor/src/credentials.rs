//! Scoped, short-lived credentials a held lease may mint.
//!
//! The supervisor mints; a sandbox never asks the daemon for anything. A token
//! minted here lives in the lease's egress vault and reaches a request only in
//! its `Authorization` header (`afr_egress`). [`LeaseMint`] is the seam the
//! vault mints through: one held lease, over the control plane.

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::credentials::{MintCredentialRequest, MintCredentialResponse};
use afr_egress::{Mint, Minted};
use afr_secrets::Secret;

use crate::client::ControlPlane;
use crate::error::{Error, Result};

/// What a mint that never reached the daemon reads as.
const UNREACHED: &str = "the daemon could not be reached to mint the credential";

/// Mints for one held lease, over the control plane.
#[derive(Debug)]
pub(crate) struct LeaseMint<'a> {
    plane: &'a ControlPlane,
    lease_id: &'a Uuid7,
}

impl<'a> LeaseMint<'a> {
    /// The mint of the lease `lease_id`, asking through `plane`.
    pub(crate) const fn new(plane: &'a ControlPlane, lease_id: &'a Uuid7) -> Self {
        Self { plane, lease_id }
    }
}

#[async_trait::async_trait]
impl Mint for LeaseMint<'_> {
    async fn mint(&self, integration: &str) -> afr_egress::Result<Minted> {
        mint(self.plane, self.lease_id, integration, None)
            .await
            .map_err(|refused| afr_egress::Error::mint_refused(refused.code(), detail(&refused)))
    }
}

/// A refusal as the model reads it: the daemon's registry code first, so a
/// bundle that reports a `UZ-REPAIR-` refusal verbatim can.
fn detail(refused: &Error) -> String {
    match (refused.refusal_code(), refused.refusal_status()) {
        (Some(code), Some(status)) => {
            format!("{}: the daemon refused the mint ({status})", code.as_str())
        }
        (None, Some(status)) => format!("the daemon refused the mint ({status})"),
        (_, None) => UNREACHED.to_owned(),
    }
}

/// Mints a credential for `integration` under the held lease.
///
/// # Errors
/// A refusal — an ungranted integration, a lease no longer held — or a
/// transport failure. Minting is not retried: the caller decides.
pub(crate) async fn mint(
    plane: &ControlPlane,
    lease_id: &Uuid7,
    integration: &str,
    scope: Option<&str>,
) -> Result<Minted> {
    let request = MintCredentialRequest {
        lease_id: Cow::Borrowed(lease_id.as_str()),
        integration: Cow::Borrowed(integration),
        scope: scope.map(Cow::Borrowed),
    };
    let body = plane.mint(&request).await?;
    let minted: MintCredentialResponse<'_> = body.decode()?;
    Ok(Minted::new(
        Secret::new(minted.token.into_owned()),
        UnixMillis::from_millis(minted.expires_at_ms),
    ))
}

#[cfg(test)]
#[path = "credentials/tests.rs"]
mod tests;
