//! One lease's outbound guard: what its tools send passes here first.

use std::borrow::Cow;
use std::sync::LazyLock;

use afd_core::clock::{Clock, SystemClock};
use afd_wire::policy::{ContextBudget, ExecutionPolicy, NetworkPolicy};
use afr_secrets::StaticSecrets;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};

use crate::admission::{Admission, Draft};
use crate::error::Result;
use crate::mint::{Mint, MintRefused, Minted};
use crate::refusal::Refusal;
use crate::transport::Outbound;
use crate::vault::Vault;

/// The policy a closed guard admits under: no host, no credential.
static CLOSED: LazyLock<ExecutionPolicy<'static>> = LazyLock::new(|| ExecutionPolicy {
    network_policy: NetworkPolicy {
        allow: Vec::new(),
        read_only: true,
        read_post_paths: Vec::new(),
    },
    tools: Vec::new(),
    secrets_map: None,
    mintable: Vec::new(),
    provider: Cow::Borrowed(""),
    api_key: Cow::Borrowed(""),
    inference_host: Cow::Borrowed(""),
    base_url: None,
    repository_binding: None,
    http_origin_policies: Vec::new(),
    context: ContextBudget {
        tool_window: 0,
        memory_checkpoint_every: 0,
        stage_chunk_threshold: 0.0,
        model: Cow::Borrowed(""),
        context_cap_tokens: 0,
    },
});

/// What a closed guard answers a mint with.
const CLOSED_MINT: &str = "this run mints no credential";

/// What every call of one lease sends through.
#[derive(Debug)]
pub struct Egress<'run> {
    admission: Admission<'run>,
    vault: Vault<'run>,
}

impl<'run> Egress<'run> {
    /// The guard for a lease under `policy`, minting through `mint`, its
    /// minted tokens expiring against `clock`.
    #[must_use]
    pub fn new(
        policy: &'run ExecutionPolicy<'run>,
        mint: &'run dyn Mint,
        clock: &'run dyn Clock,
    ) -> Self {
        Self {
            admission: Admission::new(policy),
            vault: Vault::new(mint, clock),
        }
    }

    /// A guard that admits nothing.
    #[must_use]
    pub fn closed() -> Egress<'static> {
        Egress::new(&CLOSED, &Closed, &SystemClock)
    }

    /// `draft`, admitted and with its credentials in place, ready for a
    /// transport.
    ///
    /// # Errors
    /// The policy refuses the request, a placeholder names a secret the fleet
    /// lacks, or the daemon would not mint the credential it names.
    pub async fn prepare(&mut self, draft: Draft) -> Result<Outbound, Refusal> {
        let admitted = self.admission.admit(draft)?;
        let headers = self.headers(admitted.headers).await?;
        Ok(Outbound {
            method: admitted.method,
            url: admitted.url,
            headers,
            body: admitted.body,
        })
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
    async fn headers(&mut self, written: Vec<(String, String)>) -> Result<HeaderMap, Refusal> {
        let mut headers = HeaderMap::with_capacity(written.len());
        for (name, value) in written {
            let header =
                HeaderName::from_bytes(name.as_bytes()).map_err(|_invalid| unsendable(&name))?;
            let sensitive = header == AUTHORIZATION;
            let mut sent = if sensitive {
                let filled = self.vault.fill(self.admission, &value).await?;
                HeaderValue::from_str(filled.expose())
            } else {
                HeaderValue::from_str(&value)
            }
            .map_err(|_invalid| unsendable(&name))?;
            sent.set_sensitive(sensitive);
            headers.append(header, sent);
        }
        Ok(headers)
    }
}

fn unsendable(name: &str) -> Refusal {
    Refusal::InvalidHeader {
        name: name.to_owned(),
    }
}

/// The mint of a guard that admits nothing.
#[derive(Debug)]
struct Closed;

#[async_trait::async_trait]
impl Mint for Closed {
    async fn mint(&self, _integration: &str) -> Result<Minted, MintRefused> {
        Err(MintRefused::new(CLOSED_MINT.to_owned()))
    }
}

#[cfg(test)]
#[path = "egress/tests.rs"]
mod tests;
