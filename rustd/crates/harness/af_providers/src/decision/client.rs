//! A bounded physical send, with credentials outside sandbox-visible state.

use std::time::Duration;

use af_crypto::secret::SecretString;
use af_wire::decision::{DecisionProfile, DecisionRequest, DecisionResult, REPLY_MAX_BYTES};
use futures_util::{TryStreamExt as _, future::BoxFuture};
use garde::Valid;

use super::{DecisionEvaluator, document, preflight, wire};
use crate::retry::retry_after;
use crate::{
    Result,
    error::{Error, ErrorKind, raise},
};

/// Initial connect bound for a physical decision call.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// A native reservation covers one physical request, including its full reply.
const PHYSICAL_TIMEOUT: Duration = Duration::from_secs(12);

/// One frozen decision profile and its credential, over pooled guarded HTTP.
pub struct DecisionClient {
    client: reqwest::Client,
    profile: Valid<DecisionProfile>,
    credential: SecretString,
}

impl DecisionClient {
    /// Construct the same guarded transport used by the runner's coding client.
    ///
    /// # Errors
    /// Preserves a client-construction failure without printing credentials.
    pub fn new(profile: Valid<DecisionProfile>, credential: SecretString) -> Result<Self> {
        #[expect(
            clippy::disallowed_methods,
            reason = "runner HTTP construction always passes through the shared DNS/redirect guard"
        )]
        let client = af_egress::guarded(reqwest::Client::builder())
            .connect_timeout(CONNECT_TIMEOUT)
            .retry(reqwest::retry::never())
            .build()
            .map_err(raise::client)?;
        Ok(Self::with_client(client, profile, credential))
    }

    /// Reuse a configured client; callers preserve guarded DNS, no redirects,
    /// disabled automatic retry and the three-second connection bound.
    #[must_use]
    pub const fn with_client(
        client: reqwest::Client,
        profile: Valid<DecisionProfile>,
        credential: SecretString,
    ) -> Self {
        Self {
            client,
            profile,
            credential,
        }
    }

    pub(super) async fn send(
        &self,
        profile: &Valid<DecisionProfile>,
        request: &Valid<DecisionRequest>,
        credential: &SecretString,
    ) -> Result<DecisionResult> {
        if credential.is_empty() {
            return Err(ErrorKind::DecisionPreflight {
                reason: "the decision provider credential is missing",
            }
            .into());
        }
        let endpoint = preflight::check(profile, request)?;
        let body = wire::encode(profile, request)?;
        // Validate the final header once. A builder refusal is known not to
        // have dispatched, and creates no validation-only secret buffer.
        let outbound = self
            .client
            .post(endpoint)
            .bearer_auth(credential.expose())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .timeout(PHYSICAL_TIMEOUT)
            .body(body)
            .build()
            .map_err(|source| {
                Error::from(ErrorKind::DecisionBuild {
                    source: source.without_url(),
                })
            })?;
        let response = self
            .client
            .execute(outbound)
            .await
            .map_err(|source| Error::lost(source.without_url()))?;
        if !response.status().is_success() {
            // Never collect a provider error page: it may echo the private
            // evidence. A native retry reserves a different physical call.
            return Err(ErrorKind::DecisionStatus {
                status: response.status().as_u16(),
                retry_after: retry_after(response.headers()),
            }
            .into());
        }
        let bytes = read(response).await?;
        let document = document::parse(&bytes, profile.api)?;
        Ok(wire::decode(profile, request, &document))
    }
}

impl std::fmt::Debug for DecisionClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecisionClient")
            .field("model_id", &self.profile.model_id)
            .field("profile_revision", &self.profile.revision)
            .field("api", &self.profile.api)
            .finish_non_exhaustive()
    }
}

impl DecisionEvaluator for DecisionClient {
    fn evaluate<'a>(
        &'a self,
        request: &'a Valid<DecisionRequest>,
    ) -> BoxFuture<'a, Result<DecisionResult>> {
        Box::pin(super::observe::evaluate(
            self,
            &self.profile,
            request,
            &self.credential,
        ))
    }
}

async fn read(response: reqwest::Response) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|size| size > u64::try_from(REPLY_MAX_BYTES).unwrap_or(u64::MAX))
    {
        return Err(ErrorKind::DecisionOversize.into());
    }
    let mut bytes = Vec::new();
    let mut body = response.bytes_stream();
    while let Some(chunk) = body
        .try_next()
        .await
        .map_err(|source| Error::lost(source.without_url()))?
    {
        if bytes.len().saturating_add(chunk.len()) > REPLY_MAX_BYTES {
            return Err(ErrorKind::DecisionOversize.into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
