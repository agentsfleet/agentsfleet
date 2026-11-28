//! One run's outbound guard: what its tools send passes here first.

use std::borrow::Cow;

use af_core::clock::Clock;
use af_secrets::{Scrub, StaticSecrets};
use af_wire::policy::ExecutionPolicy;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};

use crate::error::{Result, raise};
use crate::mint::Mint;
use crate::policy::{Draft, EgressPolicy};
use crate::transport::Outbound;
use crate::vault::Vault;

/// What every call of one run sends through.
#[derive(Debug)]
pub struct Egress<'run> {
    run_id: &'run str,
    policy: &'run ExecutionPolicy<'run>,
    vendor_scrub: Option<Scrub>,
    vault: Vault<'run>,
}

impl<'run> Egress<'run> {
    /// The guard for run `run_id` under `policy`, minting through
    /// `mint`, its minted tokens expiring against `clock`.
    #[must_use]
    pub fn new(
        run_id: &'run str,
        policy: &'run ExecutionPolicy<'run>,
        mint: &'run dyn Mint,
        clock: &'run dyn Clock,
    ) -> Self {
        Self {
            run_id,
            policy,
            vendor_scrub: None,
            vault: Vault::new(run_id, mint, clock),
        }
    }

    /// `draft`, allowed and with its credentials in place, ready for a
    /// transport.
    ///
    /// # Errors
    /// The policy refuses the request, a placeholder names a secret the fleet
    /// lacks, or the daemon would not mint the credential it names.
    pub async fn prepare(&mut self, draft: Draft) -> Result<Outbound> {
        let allowed = EgressPolicy::new(self.policy).allow(draft)?;
        let headers = self.headers(allowed.headers).await?;
        Ok(Outbound {
            method: allowed.method,
            url: allowed.url,
            headers,
            body: allowed.body,
            discard_redirect_body: false,
        })
    }

    /// The run this guard belongs to, for the lines its tools log.
    #[must_use]
    pub const fn run_id(&self) -> &'run str {
        self.run_id
    }

    /// `text` with every token this run minted masked.
    #[must_use]
    pub fn mask<'t>(&self, text: &'t str) -> Cow<'t, str> {
        self.vault.mask(text)
    }

    /// A handle for synchronous activity observers to mask freshly minted credentials.
    #[must_use]
    pub fn minted_mask(&self) -> crate::MintedMask {
        self.vault.masker.clone()
    }

    /// The run's static credentials, for a tool that reads its own.
    #[must_use]
    pub fn statics(&self) -> StaticSecrets<'run> {
        EgressPolicy::new(self.policy).statics()
    }

    /// Whether this fleet permits vendor-backed web tools.
    #[must_use]
    pub fn public_web(&self) -> bool {
        EgressPolicy::new(self.policy).is_public()
    }

    /// Checks the read against network and origin policy and chooses a local
    /// GET for an explicitly named host. A public unlisted host uses extract.
    ///
    /// # Errors
    /// URL, destination, origin, or fleet policy refuses the read.
    pub fn fetch_locally(&self, url: &str) -> Result<bool> {
        EgressPolicy::new(self.policy).fetch_locally(url)
    }

    /// Mask declared and minted secrets before a vendor sees input or the tool
    /// returns results. A borrowed value requires no redaction.
    ///
    /// # Errors
    /// The run's secret matcher could not be built.
    pub fn web_text<'t>(&mut self, text: &'t str) -> Result<Cow<'t, str>> {
        let scrub = match &self.vendor_scrub {
            Some(scrub) => scrub,
            None => self
                .vendor_scrub
                .insert(Scrub::new(self.policy).map_err(raise::unmaskable)?),
        };
        Ok(match scrub.text(text) {
            Cow::Borrowed(original) => self.vault.mask(original),
            Cow::Owned(clean) => Cow::Owned(self.vault.mask(&clean).into_owned()),
        })
    }

    /// The written headers as HTTP carries them, `Authorization` filled in
    /// and marked sensitive so no `Debug` of the map prints it.
    async fn headers(&mut self, written: Vec<(String, String)>) -> Result<HeaderMap> {
        let mut headers = HeaderMap::with_capacity(written.len());
        for (name, value) in written {
            let header = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_invalid| raise::invalid_header(&name))?;
            let sensitive = header == AUTHORIZATION;
            let mut sent = if sensitive {
                let filled = self
                    .vault
                    .fill(EgressPolicy::new(self.policy), &value)
                    .await?;
                HeaderValue::from_str(filled.expose())
            } else {
                HeaderValue::from_str(&value)
            }
            .map_err(|_invalid| raise::invalid_header(&name))?;
            sent.set_sensitive(sensitive);
            headers.append(header, sent);
        }
        Ok(headers)
    }
}

#[cfg(test)]
mod tests;
