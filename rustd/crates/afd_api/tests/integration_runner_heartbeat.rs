//! A runner's beat through the production router and live Postgres: what the
//! reply tells the host about the sandboxes it holds.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::borrow::Cow;

use afd_auth::scope::ScopeSet;
use afd_core::clock::UnixMillis;
use afd_crypto::entropy::Entropy;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_runner::Runners;
use afd_wire::runner::{AssignedPolicy, NetworkPolicy, RegisterRequest, SandboxTier};
use http::{Method, StatusCode};

use crate::harness::{Fleet, json_body, send};

/// The subject the live router is built for; no request here acts as it.
const SUBJECT: &str = "fixture:runner-heartbeat";

/// When the runner enrols.
const NOW: UnixMillis = UnixMillis::from_millis(1_760_000_000_000);

/// A beat listing a fleet this runner never held is answered with that fleet
/// to release: the reply carries what the store decided the host lets go of.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_heartbeat_reply_names_the_holds_to_release() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let enrolled = Runners::new(database.clone(), Entropy::new())
        .register(&enrolment(), NOW)
        .await
        .expect("the runner enrols");
    let token = enrolled.token.expose().to_owned();
    let router = Fleet::live(database.clone(), SUBJECT, ScopeSet::from_scopes(&[])).router();
    let never_held = mint_id();

    let beat = send(
        &router,
        Method::POST,
        afd_wire::paths::RUNNER_HEARTBEATS,
        Some(&token),
        &format!(r#"{{"holds":["{never_held}"]}}"#),
    )
    .await;

    assert_eq!(beat.status(), StatusCode::OK);
    let reply = json_body(beat).await;
    assert_eq!(
        reply.get("release_holds"),
        Some(&serde_json::json!([never_held])),
        "{reply}"
    );
    drop(database);
    lane.cleanup().await;
}

fn enrolment() -> RegisterRequest<'static> {
    RegisterRequest {
        host_id: Cow::Borrowed("heartbeat.fixture.test"),
        assigned_policy: AssignedPolicy {
            sandbox_tier: SandboxTier::DevNone,
            network_policy: NetworkPolicy::AllowAll,
            registry_allowlist: Vec::new(),
            worker_count: 1,
            extra_binds: Vec::new(),
        },
        labels: vec![Cow::Borrowed("heartbeat")],
    }
}
