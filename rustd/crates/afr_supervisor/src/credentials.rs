//! Scoped, short-lived credentials a held lease may mint.
//!
//! The supervisor mints; a sandbox never asks the daemon for anything. A token
//! minted here lives in the supervisor's memory and reaches a tool only through
//! the call that needs it. The tool catalog is this module's caller: a hosted
//! tool whose lease names a mintable integration asks for its token here.

use std::borrow::Cow;
use std::fmt;

use afd_core::id::Uuid7;
use afd_wire::credentials::{MintCredentialRequest, MintCredentialResponse};

use crate::client::ControlPlane;
use crate::error::Result;

/// What a minted credential prints as.
const REDACTED: &str = "Minted(redacted)";

/// A minted credential. Its `Debug` never prints the token.
#[derive(Clone, PartialEq, Eq)]
pub struct Minted {
    token: String,
    expires_at_ms: i64,
}

impl Minted {
    /// The token, for the one call that presents it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.token
    }

    /// When the daemon stops honouring it, in Unix milliseconds.
    #[must_use]
    pub const fn expires_at_ms(&self) -> i64 {
        self.expires_at_ms
    }
}

impl fmt::Debug for Minted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

/// Mints a credential for `integration` under the held lease.
///
/// # Errors
/// A refusal — an ungranted integration, a lease no longer held — or a
/// transport failure. Minting is not retried: the tool that asked decides.
pub async fn mint(
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
    Ok(Minted {
        token: minted.token.into_owned(),
        expires_at_ms: minted.expires_at_ms,
    })
}

#[cfg(test)]
#[path = "credentials/tests.rs"]
mod tests;
