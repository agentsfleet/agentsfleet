//! A fleet asks for access from inside a run, a person answers once, and the
//! answer reaches the fleet's next run.
//!
//! Each case issues a real run through the gate chain, asks with that run's
//! identity, and reads the policy the fleet's next run is delivered.

use super::seed::{seed_provider_resolution, workspace_of};
use super::*;

use std::borrow::Cow;

use af_crypto::entropy::Entropy;
use af_wire::access_verb::AccessRequest;
use afd_approval::{Decision, Inbox, KIND_INTEGRATION_GRANT, Requested, Resolution};
use afd_db::test_util::PlatformDefault;
use afd_turns::{Producer, Turns};
use serde_json::{Value, json};

/// The secret the cases ask for, and the host its body names.
pub(super) const SENTRY: &str = "sentry";
const SENTRY_HOST: &str = "sentry.io";

/// Who answers the cards.
const REVIEWER: &str = "fixture:person";

/// A fleet with one issued run, and what that run was delivered.
pub(super) struct Live {
    pub(super) ready: Ready,
    pub(super) workspace: Uuid7,
    pub(super) payload: Value,
    _default: PlatformDefault,
}

impl Live {
    pub(super) async fn issue(fixtures: &Fixtures) -> Self {
        let ready = ready(fixtures).await;
        let default = seed_provider_resolution(fixtures, &ready.fleet).await;
        let workspace = workspace_of(fixtures, &ready.fleet).await;
        let payload = delivered(fixtures, &ready).await;
        Self {
            ready,
            workspace,
            payload,
            _default: default,
        }
    }

    /// The run's request for `name`, at `scope`.
    pub(super) async fn ask(
        &self,
        fixtures: &Fixtures,
        name: &str,
        scope: Option<Value>,
    ) -> afd_fleet::Result<Requested> {
        let request = AccessRequest {
            fencing_token: self
                .payload
                .get("fencing_token")
                .and_then(Value::as_u64)
                .expect("the run's fence"),
            name: Cow::Owned(name.to_owned()),
            scope: scope.and_then(|scope| scope.as_object().cloned()),
        };
        let run = self
            .payload
            .get("run_id")
            .and_then(Value::as_str)
            .expect("the run's id");
        let run = Uuid7::parse(run).expect("an issued run id");
        let valid = garde::Unvalidated::new(request)
            .validate()
            .expect("a request inside its bounds");
        fixtures
            .plane()
            .request_access(&self.ready.runner, run, &valid, fixtures.enrolled_at())
            .await
            .map(|asked| asked.requested)
    }

    /// Ends the issued run as a report that waits would, and delivers the
    /// fleet's next head.
    pub(super) async fn next(&self, fixtures: &Fixtures) -> Option<Value> {
        settle_and_lapse(fixtures, &self.ready).await;
        let acquired =
            crate::seed::select_fleet(&fixtures.runs(), &self.ready.runner, &self.ready.fleet)
                .await?;
        drive(fixtures, &self.ready, acquired).await
    }
}

/// Assigns and delivers `ready`'s head, which must issue.
async fn delivered(fixtures: &Fixtures, ready: &Ready) -> Value {
    let assigned = assign(fixtures, ready).await;
    drive(fixtures, ready, assigned)
        .await
        .expect("the fixture fleet is delivered a run")
}

/// Settles the fleet's open turns and frees its thread.
async fn settle_and_lapse(fixtures: &Fixtures, ready: &Ready) {
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    for statement in [
        "UPDATE thread.turns SET settled_at = clock_timestamp() WHERE id = $1",
        "UPDATE thread.runs SET status = 'expired' WHERE turn_id = $1 AND status = 'active'",
    ] {
        sqlx::query(statement)
            .bind(&ready.turn_id)
            .execute(&mut *connection)
            .await
            .expect("the first run ends");
    }
    sqlx::query(
        "UPDATE thread.threads SET run_expires_at = clock_timestamp() - interval '1 second', \
         hold_expires_at = NULL WHERE fleet_id = $1",
    )
    .bind(&ready.fleet)
    .execute(&mut *connection)
    .await
    .expect("the thread is free");
}

/// Seals `body` under `name` in `workspace`'s vault, as the plane reads it.
pub(super) async fn store(fixtures: &Fixtures, workspace: &Uuid7, name: &str, body: &Value) {
    fixtures.store_secret(workspace, name, body).await;
}

/// The Sentry secret a case's workspace holds.
pub(super) async fn store_sentry(fixtures: &Fixtures, workspace: &Uuid7) {
    let body = json!({"host": SENTRY_HOST, "token": "sentry-fixture-token"});
    store(fixtures, workspace, SENTRY, &body).await;
}

/// `fleet`'s grants and the cards raised for them.
pub(super) async fn rows(fixtures: &Fixtures, fleet: &Uuid7) -> (i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM connector.integration_grants WHERE fleet_id = $1), \
         (SELECT count(*) FROM fleet.approval_gates WHERE fleet_id = $1 AND gate_type = $2)",
    )
    .bind(fleet)
    .bind(KIND_INTEGRATION_GRANT)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
    .await
    .expect("grant and card counts")
}

/// A person's answer to the card `gate`.
pub(super) async fn answer(fixtures: &Fixtures, fleet: &Uuid7, gate: &Uuid7, decision: Decision) {
    let action: String =
        sqlx::query_scalar("SELECT action_id::text FROM fleet.approval_gates WHERE id = $1")
            .bind(gate)
            .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
            .await
            .expect("the raised card");
    let turns = Turns::new(
        fixtures.database.clone(),
        fixtures.bus().clone(),
        Entropy::new(),
    );
    let resolved = Inbox::new(fixtures.database.clone(), fixtures.bus().clone(), turns)
        .resolve(&action, decision, REVIEWER, "", Some(fleet.as_str()))
        .await
        .expect("the answer is recorded");
    assert!(matches!(resolved, Resolution::Resolved(_)), "{resolved:?}");
}

/// The grant's stored `(kind, status)`.
async fn grant(fixtures: &Fixtures, fleet: &Uuid7, name: &str) -> (String, String) {
    sqlx::query_as("SELECT kind, status FROM connector.integration_grants WHERE fleet_id = $1 AND service = $2")
        .bind(fleet)
        .bind(name)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the fleet's grant")
}

/// Whether a delivered policy holds `name` in its secrets and its host in
/// its allow list.
pub(super) fn holds_sentry(payload: &Value) -> (bool, bool) {
    let secrets = payload.pointer("/policy/secrets_map");
    let held = secrets.and_then(|map| map.get(SENTRY)).is_some();
    let allowed = payload
        .pointer("/policy/network_policy/allow")
        .and_then(Value::as_array)
        .is_some_and(|allow| allow.iter().any(|host| host == SENTRY_HOST));
    (held, allowed)
}

/// Raises the Sentry card from a live run, and answers the card's id.
pub(super) async fn raised(fixtures: &Fixtures, live: &Live) -> Uuid7 {
    store_sentry(fixtures, &live.workspace).await;
    match live
        .ask(fixtures, SENTRY, None)
        .await
        .expect("the request is recorded")
    {
        Requested::Raised { gate, .. } => Some(gate),
        Requested::Pending { .. } | Requested::Covered { .. } => None,
    }
    .expect("a first request raises a card")
}

// Value: protects=a run's request raises one card carrying its turn and leaves
// the grant pending as a secret, and asking again raises no second card;
// fails_when=a repeat raises another card or the card names no turn;
// seam=`RunService::request_access` from an issued run over live Postgres;
// no_claim=the waiting settlement, which the report's commit path writes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_request_access_raises_one_card_and_waits() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    let gate = raised(&fixtures, &live).await;

    let again = live
        .ask(&fixtures, SENTRY, None)
        .await
        .expect("a repeat is answered");

    assert!(matches!(again, Requested::Pending { .. }), "{again:?}");
    assert_eq!(rows(&fixtures, &live.ready.fleet).await, (1, 1));
    assert_eq!(
        grant(&fixtures, &live.ready.fleet, SENTRY).await,
        ("secret".to_owned(), "pending".to_owned())
    );
    let turn: Option<Uuid7> =
        sqlx::query_scalar("SELECT turn_id FROM fleet.approval_gates WHERE id = $1")
            .bind(&gate)
            .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
            .await
            .expect("the card");
    assert_eq!(
        turn,
        Some(live.ready.turn_id.clone()),
        "the card carries the asking turn"
    );
    fixtures.cleanup().await;
}

// Value: protects=Approve hands the fleet's next run the secret and its host,
// and no other workspace secret; fails_when=the policy declares every vault
// entry, or the approval writes no answer turn; seam=the card's resolve and a
// second delivery over live Postgres and NATS.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_approved_request_resumes_the_turn_with_the_secret() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    assert_eq!(
        holds_sentry(&live.payload),
        (false, false),
        "nothing is held before"
    );
    let gate = raised(&fixtures, &live).await;

    answer(&fixtures, &live.ready.fleet, &gate, Decision::Approved).await;
    let answers: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM thread.turns WHERE thread_id = $1 AND producer = $2",
    )
    .bind(&live.ready.fleet)
    .bind(Producer::QuestionAnswer.as_str())
    .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
    .await
    .expect("answer turns");
    let resumed = live
        .next(&fixtures)
        .await
        .expect("the answer turn is delivered");

    assert_eq!(
        grant(&fixtures, &live.ready.fleet, SENTRY).await.1,
        "approved"
    );
    assert_eq!(answers, 1, "one answer turn resumes the fleet");
    assert_eq!(holds_sentry(&resumed), (true, true));
    let declared = resumed
        .pointer("/policy/secrets_map")
        .and_then(Value::as_object)
        .map_or(0, serde_json::Map::len);
    assert_eq!(declared, 1, "no other workspace secret is declared");
    fixtures.cleanup().await;
}

// Value: protects=Deny leaves the grant revoked and the next run without the
// secret; fails_when=a denied grant is read as access; seam=the card's resolve
// and a second delivery over live Postgres and NATS.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_denied_request_resumes_the_turn_without_access() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    let gate = raised(&fixtures, &live).await;

    answer(&fixtures, &live.ready.fleet, &gate, Decision::Denied).await;
    let resumed = live
        .next(&fixtures)
        .await
        .expect("the answer turn is delivered");

    assert_eq!(
        grant(&fixtures, &live.ready.fleet, SENTRY).await.1,
        "revoked"
    );
    assert_eq!(holds_sentry(&resumed), (false, false));
    fixtures.cleanup().await;
}
