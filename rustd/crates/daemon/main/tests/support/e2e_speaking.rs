//! What the runner messages suites share: a booted daemon pointed at a
//! loopback Slack, a sealed Slack grant, and runs taken from the seeded
//! event or from a Slack thread.
//!
//! Split from the suites by concern (RULE FLL): each suite file holds one
//! family of cases, and this holds the daemon they all speak through.
#![allow(
    dead_code,
    reason = "test support: shared by several suites, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_connector::test_util::FakeSlack;
use agentsfleetd::preflight::SLACK_API_URL_KNOB;
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

use crate::e2e::{Scenario, scenario_with};
use crate::tail::run;
use crate::verbs::{enqueue_from_thread, seal_slack_grant};
use crate::wire::{json as body_of, post, report_body, wait_for_offer};

/// A booted daemon pointed at a loopback Slack.
pub(crate) struct Speaking {
    pub(crate) scene: Scenario,
    pub(crate) http: reqwest::Client,
    pub(crate) slack: FakeSlack,
}

impl Speaking {
    pub(crate) async fn boot(supervisor: &mut Supervisor, config: &str) -> Self {
        let slack = FakeSlack::start().await;
        let api = slack.api_base();
        let scene = scenario_with(supervisor, None, &[(SLACK_API_URL_KNOB, &api)], config).await;
        seal_slack_grant(&scene).await;
        Self {
            scene,
            #[expect(
                clippy::disallowed_methods,
                reason = "a plain client calls the daemon this test booted; a proxy rule and a deadline are not what the test proves"
            )]
            http: reqwest::Client::new(),
            slack,
        }
    }

    /// Runs the seeded event, which came from no thread.
    pub(crate) async fn run_seeded(&self) -> (String, u64) {
        run(&self.scene).await
    }

    /// Settles the seeded run, then admits an event from the Slack thread
    /// and runs it: the run a line has somewhere to go from.
    pub(crate) async fn run_thread(&self) -> (String, u64) {
        let (run_id, fence) = self.run_seeded().await;
        let report = report_body(&run_id, &self.scene.turn_id, fence);
        let reported = crate::wire::report(&self.scene, &report).await;
        assert!(reported.is_ok(), "native report refused: {reported:?}");
        let event = enqueue_from_thread(&self.scene).await;
        wait_for_offer(&self.scene, &event).await
    }

    pub(crate) async fn say(&self, (run_id, fence): (&str, u64), text: &str) -> (u16, Value) {
        let path = format!("/v1/runners/me/runs/{run_id}/messages");
        let body = json!({"fencing_token": fence, "text": text});
        let response = post(&self.http, &self.scene, &path, &body).await;
        let status = response.status().as_u16();
        (status, body_of(response).await)
    }

    /// How many lines `run_id` has counted against its cap.
    pub(crate) async fn counted(&self, run_id: &str) -> i32 {
        let run_id =
            Uuid7::parse(run_id).expect("a run id the daemon answered is a canonical UUIDv7");
        let mut connection = self
            .scene
            .booted
            .database
            .acquire()
            .await
            .expect("a connection");
        sqlx::query_scalar("SELECT posted_message_count FROM thread.runs WHERE id = $1")
            .bind(&run_id)
            .fetch_one(&mut *connection)
            .await
            .expect("the run row")
    }

    /// Moves the fleet's live sequence past every run it has, as a reclaim
    /// does.
    pub(crate) async fn supersede(&self) {
        let mut connection = self
            .scene
            .booted
            .database
            .acquire()
            .await
            .expect("a connection");
        sqlx::query(
            "UPDATE thread.threads SET fencing_token = fencing_token + 1 WHERE fleet_id = $1",
        )
        .bind(&self.scene.fleet)
        .execute(&mut *connection)
        .await
        .expect("the sequence moves");
    }

    pub(crate) async fn finish(self, supervisor: Supervisor) {
        supervisor.shutdown().await;
        self.scene.cleanup().await;
    }
}
