//! Binding a channel's notice owner over live Postgres.
//!
//! Marked `#[ignore]` so `make test-unit-all` compiles and lints this without
//! a datastore; `make test-integration-rustd` runs it.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use af_core::error_code;
use af_crypto::{entropy::Entropy, secret::Kek};
use af_fleet_runtime::config::ChannelId;
use afd_db::config::DbRole;
use afd_db::test_util::{TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use afd_ingress::Ingress;

/// The chat provider the binding is keyed under.
const PROVIDER: &str = "slack";

/// A fleet that is not in the workspace binds nothing, and with no owner
/// already bound there is no fleet to answer: the caller is told the row
/// could not be read rather than handed an owner that does not exist.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn binding_a_fleet_outside_the_workspace_answers_no_owner() {
    let database = TestDatabase::shared().open(DbRole::Api, &[]).await;
    let (tenant, workspace, stranger) = (mint_id(), mint_id(), mint_id());
    let mut connection = database.acquire().await.expect("a connection");
    TenantSeed::new(&tenant)
        .insert(&mut *connection)
        .await
        .expect("the tenant seed");
    WorkspaceSeed::new(&workspace, &tenant)
        .insert(&mut *connection)
        .await
        .expect("the workspace seed");
    drop(connection);
    let vault = afd_vault::Vault::new(
        database.clone(),
        Arc::new(Kek::from_bytes([0x2a; 32])),
        Entropy::new(),
    );
    let turns = afd_turns::Turns::for_tests(database.clone(), afd_bus::test_util::unreachable());
    let ingress = Ingress::new(database.clone(), vault, turns);
    let team = format!(
        "T{}",
        workspace.as_str().replace('-', "")[20..].to_uppercase()
    );
    let channel: ChannelId = "C0NOTICE01".parse().expect("a channel id");

    let refused = ingress
        .bind_notice_owner(
            &workspace,
            PROVIDER,
            &team,
            &channel,
            &stranger,
            af_core::clock::now(),
        )
        .await
        .expect_err("a fleet outside the workspace is no owner");

    assert_eq!(refused.code(), error_code::INTERNAL_DB_QUERY);
    assert!(
        ingress
            .notice_owner(&workspace, PROVIDER, &team, &channel)
            .await
            .expect("the owner read answers")
            .is_none(),
        "nothing was bound"
    );
}
