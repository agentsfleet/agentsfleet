//! Disconnect against the grant a connect landed: both rows go, or neither.
//!
//! The vault handle and the routing rows describe one installation, and a
//! reader that finds one without the other is wrong in a way nothing reports.
//! So every case ends by asking the store whether the two still agree.

use afd_connector::grant::{Grant, Install, InstallClaim};
use serde_json::{Map, Value};

use super::*;

mod race;
mod rollback;

/// The provider these cases connect: one with inbound routing rows.
pub(super) const PROVIDER: Provider = Provider::Slack;

/// The account the first connect routes, before [`account`] scopes it.
pub(super) const ACCOUNT_A: &str = "T0FIXTUREA";

/// The account a reconnect routes in its place.
pub(super) const ACCOUNT_B: &str = "T0FIXTUREB";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_disconnect_whose_vault_delete_is_refused_keeps_its_routing_rows() {
    let vendor = FakeAtlassian::serving().await;
    let round = Round::create(&vendor).await;
    let now = UnixMillis::from_millis(NOW_MS);
    round
        .grants()
        .land(
            &round.workspace,
            PROVIDER,
            &grant(&account(&round, ACCOUNT_A)),
            now,
        )
        .await
        .expect("the first connect lands");
    // A model entry naming the grant's key is what the vault refuses a delete
    // over, before any write: `rollback` is the case that fails the second.
    let entry = reference_grant(&round).await;

    let refused = round
        .grants()
        .forget(&round.workspace, PROVIDER)
        .await
        .expect_err("the vault refuses a credential the registry still names");

    assert_eq!(
        refused.code(),
        afd_core::error_code::SECRET_REFERENCED_BY_MODEL_ENTRIES,
        "the refusal is the vault's conflict, not an operation failure: {refused}"
    );
    assert_eq!(
        routing_rows(&round).await,
        1,
        "a refused Disconnect removed the routing row and left its handle"
    );
    assert!(handle_held(&round).await, "and the handle is still there");

    drop_entry(&round, &entry).await;
    round.cleanup().await;
}

/// The handle and the routing rows agree: both present or both gone.
async fn assert_both_or_neither(round: &Round) {
    let routed = routing_rows(round).await;
    let held = handle_held(round).await;
    assert_eq!(
        routed > 0,
        held,
        "routing rows ({routed}) and the handle (held: {held}) disagree"
    );
}

/// `label` scoped to this round's workspace. Routing rows are unique by
/// provider and account across every workspace, so two cases sharing an
/// account would repoint each other's rows when the lane runs them together.
pub(super) fn account(round: &Round, label: &str) -> String {
    format!("{label}-{}", round.workspace.as_str())
}

/// A Slack grant routing `account` to whichever workspace lands it.
pub(super) fn grant(account: &str) -> Grant {
    let mut handle = Map::new();
    handle.insert("integration".to_owned(), Value::from("slack"));
    handle.insert(
        "bot_token".to_owned(),
        Value::from(format!("xoxb-{account}")),
    );
    Grant {
        handle,
        install: Some(Install {
            external_account_id: account.to_owned(),
            installed_by: SUBJECT.to_owned(),
            scopes: Vec::new(),
            claim: InstallClaim::Repoint,
        }),
    }
}

/// How many routing rows send this provider's events to the round's workspace.
async fn routing_rows(round: &Round) -> i64 {
    let mut connection = round.database.acquire().await.expect("a pooled connection");
    sqlx::query_scalar(
        "SELECT count(*) FROM core.connector_installs \
         WHERE provider = $1 AND workspace_id = $2::uuid",
    )
    .bind(PROVIDER.id())
    .bind(round.workspace.as_str())
    .fetch_one(&mut *connection)
    .await
    .expect("the routing read runs")
}

/// Whether the round's workspace still holds this provider's handle.
async fn handle_held(round: &Round) -> bool {
    let name = SecretName::parse(PROVIDER.grant_key()).expect("the grant key parses");
    round
        .vault
        .load(&round.workspace, &name)
        .await
        .expect("the vault read runs")
        .is_some()
}

/// Names the grant's key from the tenant's model registry.
async fn reference_grant(round: &Round) -> String {
    let id = mint_id();
    let mut connection = round.database.acquire().await.expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.tenant_model_entries \
           (id, tenant_id, model_id, secret_ref, created_at, updated_at) \
         VALUES ($1::uuid, $2::uuid, 'fixture-model', $3, $4, $4)",
    )
    .bind(&id)
    .bind(&round.tenant)
    .bind(PROVIDER.grant_key())
    .bind(NOW_MS)
    .execute(&mut *connection)
    .await
    .expect("the model entry inserts");
    id
}

/// Removes the entry [`reference_grant`] wrote.
async fn drop_entry(round: &Round, id: &str) {
    let mut connection = round.database.acquire().await.expect("a pooled connection");
    sqlx::query("DELETE FROM core.tenant_model_entries WHERE id = $1::uuid")
        .bind(id)
        .execute(&mut *connection)
        .await
        .expect("the model entry deletes");
}
