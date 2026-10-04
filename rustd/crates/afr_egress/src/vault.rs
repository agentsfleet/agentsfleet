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
use crate::error::{Error, Result, raise};
use crate::mint::{Mint, Minted};
use crate::placeholder::{self, SecretRef};

/// How long before its expiry a minted token is minted again, so a request
/// never leaves carrying a token the daemon is about to stop honouring.
const REFRESH_MARGIN_MILLIS: i64 = 30_000;

/// The event a mint logs under.
const EVENT_CREDENTIAL_MINTED: &str = "credential_minted";

/// One lease's credentials.
#[derive(Debug)]
pub(crate) struct Vault<'run> {
    mint: &'run dyn Mint,
    clock: &'run dyn Clock,
    minted: HashMap<String, Minted>,
    /// Tokens a re-mint replaced. The upstream may honour one until it
    /// expires, so the masker keeps every token this lease was handed.
    retired: Vec<(String, Minted)>,
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
            retired: Vec::new(),
            masker: None,
        }
    }

    /// `template` with every placeholder replaced by its value, minting what
    /// `admission` says is minted and is not yet held fresh.
    pub(crate) async fn fill(
        &mut self,
        admission: Admission<'_>,
        template: &str,
    ) -> Result<Secret> {
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
            return Err(Error::secret_not_found(missing.name, missing.field));
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
    async fn hold(&mut self, name: &str, integration: &str) -> Result<()> {
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
        let minted = self.mint.mint(integration).await?;
        // The masker is built before the token is held: a token nothing can
        // mask is never kept, so no later call sends it. It covers the token
        // this one replaces and every one replaced before it.
        let masker = Scrub::of(
            self.minted
                .iter()
                .chain(self.retired.iter().map(|(held, minted)| (held, minted)))
                .map(|(held, minted)| (held.as_str(), minted.expose()))
                .chain([(name, minted.expose())])
                .map(|(held, token)| (format!("{held}.{FIELD_TOKEN}"), token)),
        )
        .map_err(raise::unmaskable)?;
        let expires_at_ms = minted.expires_at().as_millis();
        let event = EVENT_CREDENTIAL_MINTED;
        tracing::info!(integration, expires_at_ms, event);
        if let Some(replaced) = self.minted.insert(name.to_owned(), minted) {
            self.retired.push((name.to_owned(), replaced));
        }
        self.masker = Some(masker);
        Ok(())
    }
}

#[cfg(test)]
#[path = "vault/tests.rs"]
mod tests;
