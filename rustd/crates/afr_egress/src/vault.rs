//! Where a placeholder's value comes from, for one lease.
//!
//! A static credential is read from `secrets_map`. A mintable one is minted
//! through the daemon the first time a request needs it and kept until shortly
//! before it expires, so three calls needing `github` mint once. The cache is
//! a plain map the lease owns: a lease's calls run one after another, lent
//! the lease by `&mut`, so no two can race to mint and no lock is needed.
//!
//! Every minted token joins one masker, built with the same [`Scrub`] the loop
//! applies, so a response echoing a token reads `«secret:NAME.token»` before
//! any tool hands it on.

use std::borrow::Cow;
use std::collections::HashMap;

use afd_core::clock::Clock;
use afr_secrets::{Scrub, Secret};

use crate::admission::{Admission, FIELD_TOKEN};
use crate::error::Result;
use crate::mint::{Mint, Minted};
use crate::placeholder::{self, SecretRef};
use crate::refusal::Refusal;

/// How long before its expiry a minted token is minted again, so a request
/// never leaves carrying a token the daemon is about to stop honouring.
const REFRESH_MARGIN_MILLIS: i64 = 30_000;

/// The event a mint logs under.
const EVENT_CREDENTIAL_MINTED: &str = "credential_minted";

/// What a minted token that could not join the masker is refused with.
const UNMASKABLE: &str = "the minted token could not be masked, so it was not used";

/// One lease's credentials.
#[derive(Debug)]
pub(crate) struct Vault<'run> {
    mint: &'run dyn Mint,
    clock: &'run dyn Clock,
    minted: HashMap<String, Minted>,
    /// Masks every minted token; `None` until the first mint.
    masker: Option<Scrub>,
}

impl<'run> Vault<'run> {
    /// A vault minting through `mint`, its tokens expiring against `clock`.
    pub(crate) fn new(mint: &'run dyn Mint, clock: &'run dyn Clock) -> Self {
        Self {
            mint,
            clock,
            minted: HashMap::new(),
            masker: None,
        }
    }

    /// `template` with every placeholder replaced by its value, minting what
    /// `admission` says is minted and is not yet held fresh.
    pub(crate) async fn fill(
        &mut self,
        admission: Admission<'_>,
        template: &str,
    ) -> Result<Secret, Refusal> {
        let wanted = placeholder::parse(template).unwrap_or_default();
        for secret in &wanted {
            if let Some(mintable) = admission.mints(secret.name) {
                self.hold(secret.name, &mintable.integration).await?;
            }
        }
        let statics = admission.statics();
        let value = |secret: SecretRef<'_>| match admission.mints(secret.name) {
            Some(_minted) if secret.field == FIELD_TOKEN => {
                self.minted.get(secret.name).map(Minted::expose)
            }
            Some(_minted) => None,
            None => statics.field(secret.name, secret.field),
        };
        if let Some(missing) = wanted.iter().find(|secret| value(**secret).is_none()) {
            return Err(Refusal::SecretNotFound {
                name: missing.name.to_owned(),
                field: missing.field.to_owned(),
            });
        }
        Ok(Secret::new(
            placeholder::substitute(template, value).into_owned(),
        ))
    }

    /// `text` with every minted token masked.
    pub(crate) fn mask<'t>(&self, text: &'t str) -> Cow<'t, str> {
        self.masker
            .as_ref()
            .map_or(Cow::Borrowed(text), |masker| masker.text(text))
    }

    /// Makes sure a fresh token for `name` is held, minting one if not.
    async fn hold(&mut self, name: &str, integration: &str) -> Result<(), Refusal> {
        let now = self
            .clock
            .now()
            .saturating_add_millis(REFRESH_MARGIN_MILLIS);
        if self
            .minted
            .get(name)
            .is_some_and(|held| now < held.expires_at())
        {
            return Ok(());
        }
        let minted = self.mint.mint(integration).await.map_err(|refused| {
            Refusal::CredentialMintRefused {
                detail: refused.detail().to_owned(),
            }
        })?;
        // The masker is built before the token is held: a token nothing can
        // mask is never kept, so no later call sends it.
        let masker = Scrub::of(
            self.minted
                .iter()
                .filter(|(held, _minted)| held.as_str() != name)
                .map(|(held, minted)| (held.as_str(), minted.expose()))
                .chain([(name, minted.expose())])
                .map(|(held, token)| (format!("{held}.{FIELD_TOKEN}"), token)),
        )
        .map_err(|_unbuilt| Refusal::CredentialMintRefused {
            detail: UNMASKABLE.to_owned(),
        })?;
        let expires_at_ms = minted.expires_at().as_millis();
        let event = EVENT_CREDENTIAL_MINTED;
        tracing::info!(integration, expires_at_ms, event);
        self.minted.insert(name.to_owned(), minted);
        self.masker = Some(masker);
        Ok(())
    }
}

#[cfg(test)]
#[path = "vault/tests.rs"]
mod tests;
