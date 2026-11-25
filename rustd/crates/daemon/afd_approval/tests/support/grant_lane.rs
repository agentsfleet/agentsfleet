//! What the grant suites share: the request verb over a lane, and reads of the
//! grant row, its cards and the answer turns they land.
//!
//! Every grant suite asks, answers and reads back the same three tables, so
//! the reads live here once; each test still builds the asks it makes.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_approval::{
    Decision, GrantKind, IntegrationGrants, KIND_INTEGRATION_GRANT, Requested, Resolution, Wanted,
};
use jiff::Timestamp;
use serde_json::Value;

use crate::lane::{Lane, NOW};

/// The integration whose grant takes a scope.
pub(crate) const GITHUB: &str = "github";

/// A workspace secret, whose grant takes none.
pub(crate) const SENTRY: &str = "sentry";

/// A read reach over one repository, as `RepositoryBinding::recorded` writes it.
pub(crate) const READ_SCOPE: &str = r#"{"repositories":["org/app"],"access":"read"}"#;

/// A write reach over the same repository, wider than [`READ_SCOPE`].
pub(crate) const WRITE_SCOPE: &str =
    r#"{"repositories":["org/app"],"access":"write","base":"main"}"#;

/// Who the fixture records as answering a card.
pub(crate) const REVIEWER: &str = "human:reviewer";

/// The store under test, over the lane's pool.
pub(crate) fn grants(lane: &Lane) -> IntegrationGrants {
    IntegrationGrants::new(lane.pool.clone(), Entropy::new())
}

/// Asks for `name` of `kind` on the lane's fleet, from `turn`.
pub(crate) async fn ask(
    lane: &Lane,
    name: &str,
    kind: GrantKind,
    scope: Option<&str>,
    turn: &Uuid7,
) -> Requested {
    grants(lane)
        .request(
            &lane.workspace,
            &lane.fleet,
            Wanted {
                name,
                kind,
                scope,
                turn,
            },
            NOW,
        )
        .await
        .expect("the request must run")
}

/// Answers the card for `action` on the lane's fleet.
pub(crate) async fn answer(lane: &Lane, action: &Uuid7, decision: Decision) -> Resolution {
    lane.inbox
        .resolve(
            action.as_str(),
            decision,
            REVIEWER,
            "",
            Some(lane.fleet.as_str()),
        )
        .await
        .expect("the resolve must run")
}

/// One grant row, as a request and an answer move it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GrantState {
    pub(crate) id: Uuid7,
    pub(crate) grant_type: String,
    pub(crate) status: String,
    pub(crate) scope: Option<Value>,
    pub(crate) requested_scope: Option<Value>,
    pub(crate) reason: String,
    pub(crate) approved_at: Option<Timestamp>,
    pub(crate) revoked_at: Option<Timestamp>,
    pub(crate) updated_at: Timestamp,
}

#[derive(sqlx::FromRow)]
struct GrantRow {
    id: Uuid7,
    grant_type: String,
    status: String,
    scope: Option<String>,
    requested_scope: Option<String>,
    requested_reason: String,
    approved_at: Option<jiff_sqlx::Timestamp>,
    revoked_at: Option<jiff_sqlx::Timestamp>,
    updated_at: jiff_sqlx::Timestamp,
}

/// The lane's fleet's grant for `name`, read straight from the table.
pub(crate) async fn grant(lane: &Lane, name: &str) -> GrantState {
    let row: GrantRow = sqlx::query_as(
        "SELECT id, grant_type, status, scope::text AS scope, requested_scope::text AS requested_scope,
                requested_reason, approved_at, revoked_at, updated_at
           FROM connector.integration_grants WHERE fleet_id = $1 AND service = $2",
    )
    .bind(&lane.fleet)
    .bind(name)
    .fetch_one(&mut *lane.connection().await)
    .await
    .expect("the fleet holds a grant for the name");
    GrantState {
        id: row.id,
        grant_type: row.grant_type,
        status: row.status,
        scope: row.scope.as_deref().map(json),
        requested_scope: row.requested_scope.as_deref().map(json),
        reason: row.requested_reason,
        approved_at: row.approved_at.map(jiff_sqlx::Timestamp::to_jiff),
        revoked_at: row.revoked_at.map(jiff_sqlx::Timestamp::to_jiff),
        updated_at: row.updated_at.to_jiff(),
    }
}

/// How many grant rows the lane's fleet holds.
pub(crate) async fn grant_count(lane: &Lane) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM connector.integration_grants WHERE fleet_id = $1")
        .bind(&lane.fleet)
        .fetch_one(&mut *lane.connection().await)
        .await
        .expect("the grant rows count")
}

/// One access card, as the request raised it and an answer left it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Card {
    pub(crate) gate: Uuid7,
    pub(crate) action: Uuid7,
    pub(crate) turn: Option<Uuid7>,
    pub(crate) evidence: Value,
    pub(crate) status: String,
    pub(crate) resolved_at: Option<Timestamp>,
}

#[derive(sqlx::FromRow)]
struct CardRow {
    id: Uuid7,
    action_id: Uuid7,
    turn_id: Option<Uuid7>,
    evidence: String,
    status: String,
    resolved_at: Option<jiff_sqlx::Timestamp>,
}

/// Every access card on the lane's fleet, oldest first.
pub(crate) async fn cards(lane: &Lane) -> Vec<Card> {
    let rows: Vec<CardRow> = sqlx::query_as(
        "SELECT id, action_id, turn_id, evidence::text AS evidence, status, resolved_at
           FROM fleet.approval_gates
          WHERE fleet_id = $1 AND gate_type = $2
          ORDER BY created_at, id",
    )
    .bind(&lane.fleet)
    .bind(KIND_INTEGRATION_GRANT)
    .fetch_all(&mut *lane.connection().await)
    .await
    .expect("the card rows are readable");
    rows.into_iter()
        .map(|row| Card {
            gate: row.id,
            action: row.action_id,
            turn: row.turn_id,
            evidence: json(&row.evidence),
            status: row.status,
            resolved_at: row.resolved_at.map(jiff_sqlx::Timestamp::to_jiff),
        })
        .collect()
}

/// The one card the lane's fleet holds for its first request.
pub(crate) async fn only_card(lane: &Lane) -> Card {
    let mut all = cards(lane).await;
    assert_eq!(all.len(), 1, "exactly one card: {all:?}");
    all.remove(0)
}

/// The answer an answer turn carries, and the turn it resumes.
pub(crate) async fn answer_turn(lane: &Lane, turn: &Uuid7) -> (Value, Option<String>) {
    let body = lane
        .event_column(turn.as_str(), "request_json")
        .await
        .expect("an answer turn carries a body");
    let resumes = lane.event_column(turn.as_str(), "resumes_turn_id").await;
    (json(&body), resumes)
}

/// JSON text a column returned, as a value, so a comparison ignores spacing
/// and key order.
fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("the column holds JSON")
}
