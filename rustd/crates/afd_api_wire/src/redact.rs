//! Hand-written `Debug` for the daemon-only wire types that carry a secret.
//!
//! The same rule as `afd_wire::redact`, whose helpers render the value: the
//! secret is redacted on `Debug` only and still serializes to the wire, or
//! deserializes from it unchanged. `tests/redaction.rs` proves both halves.

use std::fmt::{self, Debug, Formatter};

use afd_wire::redact::{RUNNER_TOKEN_FIELD, redacted};

use crate::admin::RunnerTokenRotatedResponse;
use crate::secret::{ReplaceSecretRequest, StoreSecretRequest};
use crate::tenant::{MintedApiKeyResponse, MintedCliCredentialResponse};

/// The field every row's identifier renders under.
const ID_FIELD: &str = "id";

/// The field a stored secret's body renders under.
const DATA_FIELD: &str = "data";

impl Debug for StoreSecretRequest<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoreSecretRequest")
            .field("name", &self.name)
            // The body IS the secret: a provider key, a password, a token.
            .field(DATA_FIELD, &redacted(self.data.get()))
            .finish()
    }
}

impl Debug for ReplaceSecretRequest<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReplaceSecretRequest")
            .field(DATA_FIELD, &redacted(self.data.get()))
            .finish()
    }
}

impl Debug for MintedApiKeyResponse<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("MintedApiKeyResponse")
            .field(ID_FIELD, &self.id)
            .field("key_name", &self.key_name)
            // Revealed once; the daemon keeps only its digest.
            .field("key", &redacted(&self.key))
            .field("created_at", &self.created_at)
            .finish()
    }
}

impl Debug for MintedCliCredentialResponse<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("MintedCliCredentialResponse")
            .field(ID_FIELD, &self.id)
            .field("credential", &redacted(&self.credential))
            .field("machine_name", &self.machine_name)
            .field("deployment", &self.deployment)
            .finish()
    }
}

impl Debug for RunnerTokenRotatedResponse<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunnerTokenRotatedResponse")
            .field(ID_FIELD, &self.id)
            .field(RUNNER_TOKEN_FIELD, &redacted(&self.runner_token))
            .finish()
    }
}
