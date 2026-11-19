//! Scoped, short-lived credentials a held run may mint.
//!
//! The supervisor mints; a sandbox never asks the daemon for anything. A token
//! minted here lives in the run's egress vault and reaches a request only in
//! its `Authorization` header (`af_egress`). [`RunMint`] is the seam the
//! vault mints through: one held run, over the control plane.

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_egress::{Mint, Minted};
use af_secrets::Secret;
use af_wire::credentials::{MintCredentialRequest, MintCredentialResponse};

use crate::client::ControlPlane;
use crate::error::{Error, Result};

/// What a mint that never reached the daemon reads as.
const UNREACHED: &str = "the daemon could not be reached to mint the credential";

/// Mints for one held run, over the control plane.
#[derive(Debug)]
pub(crate) struct RunMint<'a> {
    plane: &'a ControlPlane,
    run_id: &'a Uuid7,
}

impl<'a> RunMint<'a> {
    /// The mint of the run `run_id`, asking through `plane`.
    pub(crate) const fn new(plane: &'a ControlPlane, run_id: &'a Uuid7) -> Self {
        Self { plane, run_id }
    }
}

#[async_trait::async_trait]
impl Mint for RunMint<'_> {
    async fn mint(&self, integration: &str) -> af_egress::Result<Minted> {
        mint(self.plane, self.run_id, integration, None)
            .await
            .map_err(|refused| af_egress::Error::mint_refused(refused.code(), detail(&refused)))
    }
}

/// A refusal as the model reads it: the daemon's registry code first, so a
/// fleet whose instructions report a `UZ-REPAIR-` refusal verbatim can.
fn detail(refused: &Error) -> String {
    match (refused.refusal_code(), refused.refusal_status()) {
        (Some(code), Some(status)) => {
            format!("{}: the daemon refused the mint ({status})", code.as_str())
        }
        (None, Some(status)) => format!("the daemon refused the mint ({status})"),
        (_, None) => UNREACHED.to_owned(),
    }
}

/// Mints a credential for `integration` under the held run.
///
/// # Errors
/// A refusal — an ungranted integration, a run no longer held — or a
/// transport failure. Minting is not retried: the caller decides.
pub(crate) async fn mint(
    plane: &ControlPlane,
    run_id: &Uuid7,
    integration: &str,
    scope: Option<&str>,
) -> Result<Minted> {
    let request = MintCredentialRequest {
        run_id: Cow::Borrowed(run_id.as_str()),
        integration: Cow::Borrowed(integration),
        scope: scope.map(Cow::Borrowed),
    };
    let body = plane.mint(&request).await?;
    let minted: MintCredentialResponse<'_> = body.decode()?;
    Ok(Minted::new(
        Secret::new(minted.token.into_owned()),
        minted.expires_at,
    ))
}

#[cfg(test)]
mod tests;
