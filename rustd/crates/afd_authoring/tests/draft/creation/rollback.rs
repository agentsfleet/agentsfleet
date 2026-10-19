//! Daemon-written setup is its stream and its activation, or neither: a
//! resident whose activation fails is removed again, and a removal that fails
//! too is logged under its cause and left for an operator.

use super::super::{definition, fixture::now};
use super::fixture::Case;
use afd_authoring::SystemSetup;
use afd_core::{error_code, test_util::trace::Capture};
use sqlx::Connection as _;

/// Refuses `$event` on the workspace's fleets: the fault the proof installs.
fn refusal(name: &str, event: &str, when: &str) -> [String; 2] {
    [
        format!(
            "CREATE FUNCTION public.{name}() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN RAISE EXCEPTION 'fixture refusal'; END $$"
        ),
        format!(
            "CREATE TRIGGER {name} BEFORE {event} ON fleet.fleets FOR EACH ROW \
             WHEN ({when}) EXECUTE FUNCTION public.{name}()"
        ),
    ]
}

async fn residents(c: &Case) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM fleet.fleets WHERE workspace_id=$1::uuid")
        .bind(c.base.workspace.as_str())
        .fetch_one(&mut *c.base.connection().await)
        .await
        .expect("the workspace's fleets")
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_resident_whose_activation_fails_is_removed_unless_the_removal_fails_too() {
    // Its own database: the refusing triggers are on the shared fleets table.
    let c = Case::isolated().await;
    let definition = definition();
    let setup = SystemSetup {
        workspace: &c.base.workspace,
        definition: &definition,
    };
    let mut owner = sqlx::PgConnection::connect(&c.base.owner_url())
        .await
        .expect("the lane owner installs faults");
    for statement in refusal("refuse_activation", "UPDATE", "NEW.status = 'active'") {
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .execute(&mut owner)
            .await
            .expect("refuse activation");
    }
    c.creations
        .materialise(setup, now())
        .await
        .expect_err("the activation was refused");
    assert_eq!(residents(&c).await, 0, "the half-made resident is removed");

    for statement in refusal("refuse_removal", "DELETE", "OLD.status = 'installing'") {
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .execute(&mut owner)
            .await
            .expect("refuse removal");
    }
    let capture = Capture::install();
    c.creations
        .materialise(setup, now())
        .await
        .expect_err("the activation was refused again");
    let logged = capture.only("system_setup_rollback_failed");
    drop(capture);
    assert_eq!(
        logged.field("error_code"),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert_eq!(
        logged.field("cause_error_code"),
        Some(error_code::INTERNAL_DB_QUERY.as_str())
    );
    assert_eq!(residents(&c).await, 1, "the resident stays for an operator");
    for statement in [
        "DROP TRIGGER refuse_removal ON fleet.fleets",
        "DROP TRIGGER refuse_activation ON fleet.fleets",
    ] {
        sqlx::query(statement)
            .execute(&mut owner)
            .await
            .expect("remove the faults before cleanup");
    }
    owner.close().await.expect("release the owner session");
    c.cleanup().await;
}
