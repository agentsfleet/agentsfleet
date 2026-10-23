//! An initial run the admission ledger cannot record, for a reason that is
//! not capacity, fails the activation that carried it; the fleet stays
//! installing and the same key finishes it once the ledger answers.

use super::super::{fixture::now, mint};
use super::fixture::{Case, NO_ADMISSIONS, check};
use afd_admission::Admissions;
use afd_authoring::{Creations, types::Start};
use afd_core::error_code;
use afd_crypto::entropy::Entropy;
use sqlx::Connection as _;

/// The initial run every case asks for.
fn run_now() -> Start {
    Start::RunNow {
        message: "Run once setup finishes".to_owned(),
    }
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn an_admission_whose_identity_cannot_be_drawn_fails_the_activation() {
    let c = Case::create().await;
    let (entropy, control) = Entropy::new_mocked();
    let admissions = Admissions::new(c.base.db.clone(), c.queue.clone(), entropy);
    let creations = Creations::new(c.setup.clone(), admissions);
    let (_, request) = c.request("entropy-fault", run_now()).await;
    let key = mint();
    let first = c.save(&key, request.clone()).await;
    assert_eq!(first.status, "installing", "unfunded, so setup stops early");
    c.fund().await;
    control.fail_next();
    let refused = creations
        .save(c.base.access(), &key, request.clone(), now())
        .await
        .expect_err("no admission identity, no activation");
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
    let stalled = c.creations.read(c.base.access(), &first.id).await;
    assert_eq!(
        check(&stalled.expect("retained fleet"), "activation"),
        "pending"
    );
    assert_eq!(c.admissions(&first.id).await, NO_ADMISSIONS);
    assert_eq!(
        creations
            .save(c.base.access(), &key, request, now())
            .await
            .expect("the next draw answers")
            .status,
        "active"
    );
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn an_admission_insert_the_ledger_refuses_rolls_the_activation_back() {
    // Its own database: the refusing trigger is on the shared admissions table.
    let c = Case::isolated().await;
    let (_, request) = c.request("ledger-fault", run_now()).await;
    let key = mint();
    let fleet = c.save(&key, request.clone()).await.id;
    let mut owner = sqlx::PgConnection::connect(&c.base.owner_url())
        .await
        .expect("the lane owner installs the fault");
    for statement in [
        "CREATE FUNCTION public.refuse_admission() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'fixture refusal'; END $$",
        "CREATE TRIGGER refuse_admission BEFORE INSERT ON fleet.admissions \
         FOR EACH ROW EXECUTE FUNCTION public.refuse_admission()",
    ] {
        sqlx::query(statement)
            .execute(&mut owner)
            .await
            .expect("install the ledger refusal");
    }
    c.fund().await;
    let refused = c
        .creations
        .save(c.base.access(), &key, request, now())
        .await
        .expect_err("the run could not be recorded");
    assert_eq!(refused.code(), error_code::INTERNAL_DB_QUERY);
    let stalled = c.creations.read(c.base.access(), &fleet).await;
    let stalled = stalled.expect("retained fleet");
    assert_eq!(
        stalled.status, "installing",
        "the activation rolled back with it"
    );
    assert_eq!(c.admissions(&fleet).await, NO_ADMISSIONS);
    owner.close().await.expect("release the owner session");
    c.cleanup().await;
}
