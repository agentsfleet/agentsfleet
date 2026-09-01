//! What a stored self-managed credential must prove before it is activated.
//!
//! Split from [`super::managed`] at the file cap. The seam is the one the two
//! halves already had: `managed` RESOLVES a credential the daemon will dial,
//! and this vets a body a tenant handed us — the refusals here are answered to
//! a client, so they are values rather than errors and each maps to a registry
//! code at the handler.

use super::endpoint::{self, Rejection};
use super::managed::{Credential, FIELD_PROVIDER};
use super::resolved::Dialled;
use crate::error::{provider_endpoint, provider_malformed};
use crate::provider::resolved::SecretString;

/// One stored credential, parsed and with its endpoint ruled on.
///
/// What resolution and ACTIVATION agree about a credential, extracted so there
/// is one parser and one endpoint ruling rather than two that could come to
/// disagree about the same bytes. What they do with it differs: resolution
/// applies the key rule and dials; activation reads the provider and the
/// fallback model, and never looks at the key at all.
pub(super) struct Vetted {
    /// The provider this credential is for. Never empty.
    pub(super) provider: Box<str>,
    /// The credential's own model, for a caller with no better source.
    pub(super) model: Option<Box<str>>,
    /// The validated custom endpoint, present only for the compatible
    /// provider. `Some` IS the compatible provider — the guard has refused
    /// every other pairing by this point.
    pub(super) dialled: Option<Dialled>,
    /// The bearer key, still unjudged: whether absence is permitted is the
    /// caller's rule, not the parse's.
    pub(super) api_key: Option<SecretString>,
}

/// Why a credential body did not vet.
///
/// TYPED rather than folded into [`crate::Error`], because activation
/// DISCRIMINATES on it to pick a registry code — the carve-out
/// `RUST_ERROR_STANDARD` names. Keeping the guard's [`Rejection`] as itself
/// rather than as the string it renders to means a refusal added to the guard
/// later cannot be silently absorbed into the malformed arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refused {
    /// The body is not a credential this daemon can read.
    Malformed,
    /// The endpoint guard refused the provider/base-url pairing.
    Endpoint(Rejection),
}

impl From<Refused> for crate::Error {
    /// So the RESOLUTION path keeps writing `?` and answering the crate error,
    /// while activation matches the typed value.
    fn from(refused: Refused) -> Self {
        match refused {
            Refused::Malformed => provider_malformed(FIELD_PROVIDER),
            Refused::Endpoint(rejection) => provider_endpoint(rejection.as_str()),
        }
    }
}

/// Parses one credential body and rules on its endpoint.
///
/// Endpoint first, BEFORE the key is looked at: a hostile or mismatched
/// endpoint fails while the credential is still just bytes, which is the
/// ordering `probeSelfManagedSecret` chose and the reason it gave — nothing
/// owned is built around a URL that will be refused.
///
/// # Errors
/// Reports a body that is not a credential object, a blank or missing
/// provider, and an endpoint the guard refused — each as a [`Refused`] the
/// caller may discriminate on.
pub(super) fn vet(body: &[u8]) -> Result<Vetted, Refused> {
    let credential: Credential =
        super::credential(body, FIELD_PROVIDER).map_err(|_unreadable| Refused::Malformed)?;
    if credential.provider.is_empty() {
        return Err(Refused::Malformed);
    }

    // The host travels with the URL from here, because `resolve` already
    // derived it to make its SSRF ruling — see [`Dialled`].
    let dialled: Option<Dialled> =
        endpoint::resolve(&credential.provider, credential.base_url.as_deref())
            .map_err(Refused::Endpoint)?
            .map(|endpoint| Dialled {
                base_url: endpoint.url.into(),
                inference_host: endpoint.host,
            });

    Ok(Vetted {
        provider: credential.provider,
        model: credential.model,
        dialled,
        api_key: credential.api_key,
    })
}
