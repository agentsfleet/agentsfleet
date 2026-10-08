//! Hand-written `Debug` for the daemon-only wire types that carry a secret.
//!
//! The same rule as `afd_wire::redact`, whose helpers render the value: the
//! secret is redacted on `Debug` only and still serializes to the wire.
//! `tests/redaction.rs` proves both halves.

use std::fmt::{self, Debug, Formatter};

use afd_wire::redact::{RUNNER_TOKEN_FIELD, redacted};

use crate::admin::RunnerTokenRotatedResponse;

impl Debug for RunnerTokenRotatedResponse<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunnerTokenRotatedResponse")
            .field("id", &self.id)
            .field(RUNNER_TOKEN_FIELD, &redacted(&self.runner_token))
            .finish()
    }
}
