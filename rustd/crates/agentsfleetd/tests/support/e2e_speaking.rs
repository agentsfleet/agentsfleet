//! What the runner messages suites share: a booted daemon pointed at a
//! loopback Slack, a sealed Slack grant, and leases taken from the seeded
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

use afd_connector::test_util::FakeSlack;
use agentsfleetd::preflight::SLACK_API_URL_KNOB;
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

use crate::e2e::{Scenario, scenario_with};
use crate::tail::lease;
use crate::verbs::{enqueue_from_thread, seal_slack_grant};
use crate::wire::{json as body_of, poll_for_lease, post, report_body};

/// A booted daemon pointed at a loopback Slack.
pub(crate) struct Speaking {
    pub(crate) run: Scenario,
    pub(crate) http: reqwest::Client,
    pub(crate) slack: FakeSlack,
}

impl Speaking {
    pub(crate) async fn boot(supervisor: &mut Supervisor, config: &str) -> Self {
        let slack = FakeSlack::start().await;
        let api = slack.api_base();
        let run = scenario_with(supervisor, None, &[(SLACK_API_URL_KNOB, &api)], config).await;
        seal_slack_grant(&run).await;
        Self {
            run,
            http: reqwest::Client::new(),
            slack,
        }
    }

    /// Leases the seeded event, which came from no thread.
    pub(crate) async fn lease_seeded(&self) -> (String, u64) {
        lease(&self.http, &self.run).await
    }

    /// Settles the seeded lease, then admits an event from the Slack thread
    /// and leases it: the lease a line has somewhere to go from.
    pub(crate) async fn lease_thread(&self) -> (String, u64) {
        let (lease_id, fence) = self.lease_seeded().await;
        let report = report_body(&lease_id, &self.run.event_id, fence);
        let reported = post(&self.http, &self.run, "/v1/runners/me/reports", &report).await;
        assert_eq!(reported.status().as_u16(), 200, "the seeded lease settles");
        let event = enqueue_from_thread(&self.run).await;
        poll_for_lease(&self.http, &self.run, &event).await
    }

    pub(crate) async fn say(&self, (lease_id, fence): (&str, u64), text: &str) -> (u16, Value) {
        let path = format!("/v1/runners/me/leases/{lease_id}/messages");
        let body = json!({"fencing_token": fence, "text": text});
        let response = post(&self.http, &self.run, &path, &body).await;
        let status = response.status().as_u16();
        (status, body_of(response).await)
    }

    /// How many lines `lease_id` has counted against its cap.
    pub(crate) async fn counted(&self, lease_id: &str) -> i32 {
        let mut connection = self
            .run
            .booted
            .database
            .acquire()
            .await
            .expect("a connection");
        sqlx::query_scalar("SELECT messages_posted FROM fleet.runner_leases WHERE id = $1::uuid")
            .bind(lease_id)
            .fetch_one(&mut *connection)
            .await
            .expect("the lease row")
    }

    /// Moves the fleet's live sequence past every lease it has, as a reclaim
    /// does.
    pub(crate) async fn supersede(&self) {
        let mut connection = self
            .run
            .booted
            .database
            .acquire()
            .await
            .expect("a connection");
        sqlx::query(
            "UPDATE fleet.runner_affinity SET fencing_seq = fencing_seq + 1 WHERE fleet_id = $1::uuid",
        )
        .bind(&self.run.fleet)
        .execute(&mut *connection)
        .await
        .expect("the sequence moves");
    }

    pub(crate) async fn finish(self, supervisor: Supervisor) {
        supervisor.shutdown().await;
        self.run.cleanup().await;
    }
}
