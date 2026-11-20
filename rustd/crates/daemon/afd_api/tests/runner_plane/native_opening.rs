//! Native opening metadata and shared stream capacity use the production router.

use super::fixture::{self as stream_fixture, Fixture, SUBJECT};
use crate::harness::{self, Fleet, native::Session};
use af_core::{error_code, id::Uuid7};
use afd_auth::{
    credential::Presented,
    directory::Digest,
    scope::{Scope, ScopeSet},
};
use afd_db::test_util::{RunnerPolicy, RunnerSeed, mint_id};
use http::{StatusCode, header::RETRY_AFTER};
use serde_json::json;

struct Routed {
    fixture: Fixture,
    runner: Uuid7,
    token: String,
}

impl Routed {
    async fn create() -> Self {
        let fixture = Fixture::create().await;
        fixture.seed().await;
        let runner = mint_id();
        let token = format!("agt_r{}{}", mint_id(), mint_id()).replace('-', "");
        let digest = Digest::of(&Presented::new(&token).expect("fixture runner credential"));
        let mut connection = fixture
            .database
            .acquire()
            .await
            .expect("runner seed connection");
        RunnerSeed::new(
            &runner,
            afd_state::sql::RUNNER_STATUS_ACTIVE,
            RunnerPolicy {
                sandbox_tier: "dev_none",
                network_policy: "deny_all_egress",
            },
        )
        .token_hash(digest.as_str())
        .insert(&mut *connection)
        .await
        .expect("enrolled runner record");
        drop(connection);
        Self {
            fixture,
            runner,
            token,
        }
    }

    fn services(&self) -> Fleet {
        Fleet::live(
            self.fixture.database.clone(),
            SUBJECT,
            ScopeSet::from_scopes(&Scope::ALL),
        )
        .with_owned_workspace(self.fixture.workspace.clone())
        .with_native_setup(self.fixture.database.clone())
    }

    async fn cleanup(self) {
        let mut connection = self
            .fixture
            .database
            .acquire()
            .await
            .expect("runner cleanup connection");
        sqlx::query("DELETE FROM runner.runners WHERE id=$1")
            .bind(&self.runner)
            .execute(&mut *connection)
            .await
            .expect("remove this runner");
        drop(connection);
        self.fixture.cleanup().await;
    }
}

/// Dimension 8.2: the runner protocol is v4 alone. A runner speaking v2, the
/// revision that kept the daemon-side setup run, v3, which no runner shipped,
/// or a version no daemon knows is refused at hello with `UZ-RUN-023`: it
/// owns no connection, so it is assigned no run.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_runner_on_another_protocol_version_is_refused() {
    let enrolled = Routed::create().await;
    let router = enrolled.services().router();
    for version in [
        "agentsfleet.runner.v2",
        "agentsfleet.runner.v3",
        "agentsfleet.runner.v9",
    ] {
        let response = Session::opening(&router, &enrolled.token, version).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{version}");
        let document = harness::json_body(response).await;
        assert_eq!(
            document.get("error_code"),
            Some(&json!(error_code::RUN_PROTOCOL_UNSUPPORTED.as_str())),
            "{version}"
        );
    }
    assert_eq!(error_code::RUN_PROTOCOL_UNSUPPORTED.as_str(), "UZ-RUN-023");
    let mut connection = enrolled
        .fixture
        .database
        .acquire()
        .await
        .expect("owner observer");
    let owner: Option<Uuid7> =
        sqlx::query_scalar("SELECT connection_id FROM runner.runners WHERE id=$1")
            .bind(&enrolled.runner)
            .fetch_one(&mut *connection)
            .await
            .expect("the refused opening leaves the enrolled record readable");
    assert!(
        owner.is_none(),
        "an unsupported protocol cannot acquire ownership"
    );
    let assigned: i64 = sqlx::query_scalar("SELECT count(*) FROM thread.runs WHERE runner_id=$1")
        .bind(&enrolled.runner)
        .fetch_one(&mut *connection)
        .await
        .expect("the runner's runs read");
    assert_eq!(assigned, 0, "a refused runner is assigned no run");
    drop(connection);
    drop(router);
    enrolled.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_stream_past_the_ceiling_is_refused_with_retry() {
    let enrolled = Routed::create().await;
    let bus = harness::bus().await;
    let router = enrolled
        .services()
        .with_stream_capacity(bus.clone(), 1)
        .router();
    let body = stream_fixture::open_stream(&router, &enrolled.fixture).await;
    let response = Session::opening(&router, &enrolled.token, af_wire::protocol::VERSION).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let retry = response
        .headers()
        .get(RETRY_AFTER)
        .expect("a full shared stream ceiling advertises retry")
        .to_str()
        .expect("retry delay is readable")
        .parse::<u64>()
        .expect("retry delay is whole seconds");
    assert!(retry > 0, "the client receives a positive retry delay");
    let document = harness::json_body(response).await;
    assert_eq!(
        document.get("error_code"),
        Some(&json!(error_code::SSE_STREAM_CAP.as_str()))
    );
    drop(body);
    drop(router);
    bus.shutdown().await.expect("the lane bus closes");
    enrolled.cleanup().await;
}
