//! Dimension 4.1 — which credential class a presented value routes to, and
//! what each class does with it.
//!
//! Routing and resolution are one procedure, so one file covers both, and
//! the properties worth pinning are the ones an `if`-chain gets by accident:
//! that markers cannot shadow one another, that a plane refuses a foreign class
//! before it costs a round trip, and that a deployment with no identity
//! provider still resolves the classes that never needed one.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "bearer_prefix_routing/classification.rs"]
mod classification;
#[path = "bearer_prefix_routing/credential.rs"]
mod credential;
#[path = "bearer_prefix_routing/resolution.rs"]
mod resolution;
#[path = "bearer_prefix_routing/session_token.rs"]
mod session_token;

use afd_auth::credential::Presented;
use afd_auth::directory::{CredentialRecord, Liveness};
use afd_auth::principal::Subject;
use afd_core::id::Uuid7;

const TENANT: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";
const RUNNER: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a6c";
const WORKSPACE: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a7d";
const SUBJECT: &str = "user_2aXyTest";
/// A body of the shape every minter produces: 32 random bytes as lower-case hex.
const BODY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn uuid(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("a valid v7 identifier")
}

fn subject() -> Subject {
    Subject::new(SUBJECT).expect("a non-blank subject")
}

fn present(raw: &str) -> Presented {
    Presented::new(raw).expect("a non-blank credential")
}

fn live_person() -> CredentialRecord {
    CredentialRecord::Person {
        tenant: uuid(TENANT),
        subject: subject(),
        live: Liveness::Live,
    }
}

fn revoked_person() -> CredentialRecord {
    CredentialRecord::Person {
        tenant: uuid(TENANT),
        subject: subject(),
        live: Liveness::Revoked,
    }
}

fn live_machine(degraded: bool) -> CredentialRecord {
    CredentialRecord::Machine {
        runner: uuid(RUNNER),
        degraded,
        live: Liveness::Live,
    }
}

/// Runs a future to completion on a current-thread runtime.
///
/// The crate is not concurrent, so a multi-thread runtime would be claiming a
/// property nothing here has.
fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a current-thread runtime")
        .block_on(future)
}
