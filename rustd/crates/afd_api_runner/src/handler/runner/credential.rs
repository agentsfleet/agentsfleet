//! `POST /v1/runners/me/credentials/mint` — a credential, at the moment a tool
//! needs one.
//!
//! # The body names no workspace, and that is the design
//!
//! A sandboxed child asks its runner for a token mid-run, and the runner
//! forwards the ask here. The child is the least trusted thing in the system —
//! it reads whatever a webhook payload said — so the request carries a
//! `lease_id` and an integration name and nothing else. Which workspace's vault
//! is opened is resolved SERVER-SIDE from that lease, scoped to the presenting
//! runner, which leaves a prompt-injected child nothing to forge: a foreign or
//! stale lease id resolves to no row, never to another tenant's credential.
//!
//! # What this layer does not decide
//!
//! Any of it. The grant gate, the write gate, the exchange and the mapping from
//! an outcome to a registry code all live in `afd_fleet`, where they can be
//! proven without an HTTP server. This function supplies the identity, reads a
//! body, and renders what comes back — which is the same division every verb on
//! this plane keeps.

use std::sync::Arc;

use afd_wire::credentials::{MintCredentialRequest, MintCredentialResponse};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};

use crate::auth::RunnerIdentity;
use crate::handler::{malformed, refuse};
use crate::services::{Leasing as _, Services};

/// The scoped event a failed mint is logged under.
const EVENT: &str = "runner_credential_mint_failed";

/// The refusal a body this daemon cannot read earns.
const DETAIL_MALFORMED: &str = "Malformed mint request body";

/// Mints one short-lived credential for the child behind this runner.
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    body: Bytes,
) -> Response {
    // Borrowed out of `body`: the lease id and the integration name both go
    // straight into a statement's parameters, so neither needs owning.
    let Ok(request) = afd_core::json::object_from_slice::<MintCredentialRequest<'_>>(&body) else {
        return malformed(DETAIL_MALFORMED);
    };

    match services
        .leases()
        .mint(runner.id(), &request, services.now())
        .await
    {
        // The ONE place the token is written, and it is written into a response
        // body — never a log line, never a frame (RULE VLT). `Minted` zeroes it
        // when this borrow ends.
        Ok(minted) => Json(MintCredentialResponse {
            token: minted.token.as_str().into(),
            expires_at_ms: minted.expires_at_ms,
        })
        .into_response(),
        // Every refusal already carries its own registry code and sentence, so
        // there is no matching here — which is what keeps a new outcome from
        // needing an edit in two crates.
        Err(error) => refuse(&error, EVENT),
    }
}
