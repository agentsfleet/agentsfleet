//! One lease's outbound guard: what its tools send passes here first.

use std::borrow::Cow;

use afd_core::clock::Clock;
use afd_wire::policy::ExecutionPolicy;
use afr_secrets::StaticSecrets;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};

use crate::admission::{Admission, Draft};
use crate::error::{Result, raise};
use crate::mint::Mint;
use crate::transport::Outbound;
use crate::vault::Vault;

/// What every call of one lease sends through.
#[derive(Debug)]
pub struct Egress<'run> {
    lease_id: &'run str,
    admission: Admission<'run>,
    vault: Vault<'run>,
}

impl<'run> Egress<'run> {
    /// The guard for lease `lease_id` under `policy`, minting through
    /// `mint`, its minted tokens expiring against `clock`.
    #[must_use]
    pub fn new(
        lease_id: &'run str,
        policy: &'run ExecutionPolicy<'run>,
        mint: &'run dyn Mint,
        clock: &'run dyn Clock,
    ) -> Self {
        Self {
            lease_id,
            admission: Admission::new(policy),
            vault: Vault::new(lease_id, mint, clock),
        }
    }

    /// `draft`, admitted and with its credentials in place, ready for a
    /// transport.
    ///
    /// # Errors
    /// The policy refuses the request, a placeholder names a secret the fleet
    /// lacks, or the daemon would not mint the credential it names.
    pub async fn prepare(&mut self, draft: Draft) -> Result<Outbound> {
        let admitted = self.admission.admit(draft)?;
        let headers = self.headers(admitted.headers).await?;
        Ok(Outbound {
            method: admitted.method,
            url: admitted.url,
            headers,
            body: admitted.body,
        })
    }

    /// The lease this guard belongs to, for the lines its tools log.
    #[must_use]
    pub const fn lease_id(&self) -> &'run str {
        self.lease_id
    }

    /// `text` with every token this lease minted masked.
    #[must_use]
    pub fn mask<'t>(&self, text: &'t str) -> Cow<'t, str> {
        self.vault.mask(text)
    }

    /// The lease's static credentials, for a tool that reads its own.
    #[must_use]
    pub const fn statics(&self) -> StaticSecrets<'run> {
        self.admission.statics()
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
                let filled = self.vault.fill(self.admission, &value).await?;
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
#[path = "egress/tests.rs"]
mod tests;
