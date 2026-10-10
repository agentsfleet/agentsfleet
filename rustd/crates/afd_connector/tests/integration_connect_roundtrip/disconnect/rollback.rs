//! A Disconnect whose second write fails takes its first back with it.
//!
//! The vault delete is a Disconnect's first write and the routing delete its
//! second. Refusing the second, after the first has run, is the one failure
//! that tells "one transaction" from "two writes in order": a refused vault
//! delete stops before anything is written, so it passes either way.

use super::*;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_disconnect_whose_routing_delete_fails_keeps_its_handle() {
    let vendor = FakeAtlassian::serving().await;
    let (round, migrator) = Round::create_private(&vendor).await;
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
    refuse_routing_deletes(&migrator).await;

    round
        .grants()
        .forget(&round.workspace, PROVIDER)
        .await
        .expect_err("the routing delete is refused");

    assert!(
        handle_held(&round).await,
        "the vault delete rolled back with the refused routing delete"
    );
    assert_eq!(
        routing_rows(&round).await,
        1,
        "and the routing row is still there"
    );

    round.drop_private().await;
}

/// Makes every routing-row delete in `migrator`'s database fail, as a write
/// the database refuses mid-transaction would. The database is the case's
/// own, so no other case sees the trigger.
async fn refuse_routing_deletes(migrator: &Db) {
    let mut connection = migrator.acquire().await.expect("a migrator connection");
    sqlx::raw_sql(
        "CREATE FUNCTION core.refuse_routing_delete() RETURNS trigger LANGUAGE plpgsql AS \
           'BEGIN RAISE EXCEPTION ''a routing delete refused on demand''; END'; \
         CREATE TRIGGER refuse_routing_delete BEFORE DELETE ON core.connector_installs \
           FOR EACH ROW EXECUTE FUNCTION core.refuse_routing_delete()",
    )
    .execute(&mut *connection)
    .await
    .expect("the refusing trigger installs");
}
