//! A first message makes an active fleet, its thread and its first turn, once.

use std::sync::Arc;

use af_core::error_code;
use afd_api_wire::fleet::CreateFleetRequest;
use afd_db::test_util::statements::transactions_begun;
use tokio::sync::Barrier;

use super::fixture::{Fixture, valid};
use super::mint;

/// How many callers race one create.
const CALLERS: usize = 100;

// Value: protects=a first message writes the fleet, its thread and its queued
// turn in one transaction; fails_when=the turn is written in a second
// transaction, or the fleet is born anything but active; seam=`Creations::create`
// over a database of its own, its transactions counted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_first_message_creates_an_active_fleet_with_its_turn() {
    let f = Fixture::isolated().await;
    let owner = f.owner_db().await;
    let before = transactions_begun(&owner).await;

    let created = f
        .creations
        .create(
            f.access(),
            &valid(f.request(&mint())),
            super::fixture::now(),
        )
        .await
        .expect("a first message makes a fleet");

    let begun = transactions_begun(&owner).await - before;
    assert_eq!(
        begun, 1,
        "the fleet, its thread and its turn commit together"
    );
    assert!(!created.replayed);
    assert_eq!(created.fleet.status, "active");
    assert_eq!(created.fleet.model_ref, f.model.to_string());
    let thread: af_core::id::Uuid7 =
        sqlx::query_scalar("SELECT id FROM thread.threads WHERE fleet_id = $1")
            .bind(&created.fleet.id)
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("the fleet's thread");
    assert_eq!(thread, created.fleet.id, "the thread's id is the fleet's");
    let actor = afd_turns::view::steer_actor(f.owner.as_str());
    assert_eq!(
        f.turns(&created.fleet.id).await,
        [(actor, af_core::turn::status::QUEUED.to_owned())]
    );
    assert_eq!(f.counts().await, (1, 1, 1));
    owner.close().await;
    f.cleanup().await;
}

// Value: protects=a create retried under one operation makes one fleet and
// one turn however many callers race it; fails_when=the replay receipt is
// written after the fleet, or a lost race writes a second row;
// seam=`Creations::create` from a barrier over live Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_repeated_create_answers_the_same_fleet() {
    let f = Fixture::create().await;
    let request = Arc::new(valid(f.request(&mint())));
    let barrier = Arc::new(Barrier::new(CALLERS));
    let mut callers = tokio::task::JoinSet::new();
    for _ in 0..CALLERS {
        let (creations, request, barrier) = (
            f.creations.clone(),
            Arc::clone(&request),
            Arc::clone(&barrier),
        );
        let (workspace, subject) = (f.workspace.clone(), f.owner.to_string());
        callers.spawn(async move {
            barrier.wait().await;
            let access = afd_setup::Access {
                workspace: &workspace,
                subject: &subject,
            };
            creations
                .create(access, &request, super::fixture::now())
                .await
                .expect("every caller is answered the fleet")
        });
    }
    let answers = callers.join_all().await;

    let first = answers.first().expect("a hundred answers");
    assert!(
        answers
            .iter()
            .all(|answer| answer.fleet.id == first.fleet.id)
    );
    assert_eq!(
        answers.iter().filter(|answer| answer.replayed).count(),
        CALLERS - 1
    );
    assert_eq!(f.counts().await, (1, 1, 1));
    f.cleanup().await;
}

// Value: protects=an operation names one request: the same key with other
// text is refused and writes nothing; fails_when=the replay digest drops the
// message; seam=`Creations::create` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_repeated_create_with_other_text_is_refused() {
    let f = Fixture::create().await;
    let operation = mint();
    f.creations
        .create(
            f.access(),
            &valid(f.request(&operation)),
            super::fixture::now(),
        )
        .await
        .expect("the first create");
    let changed = CreateFleetRequest {
        message: "Something else entirely.".to_owned(),
        ..f.request(&operation)
    };

    let refused = f
        .creations
        .create(f.access(), &valid(changed), super::fixture::now())
        .await
        .expect_err("another request under the same operation");

    assert_eq!(refused.code(), error_code::SETUP_REPLAY);
    assert_eq!(f.counts().await, (1, 1, 1));
    f.cleanup().await;
}

// Value: protects=a retried create whose fleet was deleted since answers gone
// rather than making the fleet again; fails_when=the replay finds no fleet and
// writes a new one; seam=`Creations::{create, delete}` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_repeated_create_after_delete_is_gone() {
    let f = Fixture::create().await;
    let request = valid(f.request(&mint()));
    let created = f
        .creations
        .create(f.access(), &request, super::fixture::now())
        .await
        .expect("the first create");
    f.creations
        .delete(f.access(), &created.fleet.id, &created.fleet.etag)
        .await
        .expect("the owner deletes the fleet");

    let gone = f
        .creations
        .create(f.access(), &request, super::fixture::now())
        .await
        .expect_err("the replayed fleet is gone");

    assert_eq!(gone.code(), error_code::SETUP_GONE);
    assert_eq!(f.counts().await, (0, 0, 0));
    f.cleanup().await;
}

// Value: protects=a create with no model of its own and none selected is
// refused before any row is written; fails_when=the fleet is written with no
// model, or the refusal leaves a fleet, thread or turn; seam=`Creations::create`
// over a database of its own, holding no platform default.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_create_with_no_usable_model_writes_nothing() {
    let f = Fixture::isolated().await;
    let request = CreateFleetRequest {
        model_ref: None,
        ..f.request(&mint())
    };

    let refused = f
        .creations
        .create(f.access(), &valid(request), super::fixture::now())
        .await
        .expect_err("nothing resolves a model");

    assert_eq!(refused.code(), error_code::SETUP_MODEL);
    assert_eq!(f.counts().await, (0, 0, 0));
    f.cleanup().await;
}

// Value: protects=a name another fleet of the workspace holds refuses the
// create and writes nothing; fails_when=the explicit name is silently derived
// anew, or the refusal leaves a row; seam=`Creations::create` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_taken_name_refuses_the_create() {
    let f = Fixture::create().await;
    let named = |operation| CreateFleetRequest {
        name: Some("Support triage".to_owned()),
        ..f.request(&operation)
    };
    f.creations
        .create(f.access(), &valid(named(mint())), super::fixture::now())
        .await
        .expect("the first fleet takes the name");

    let refused = f
        .creations
        .create(f.access(), &valid(named(mint())), super::fixture::now())
        .await
        .expect_err("the name is taken");

    assert_eq!(refused.code(), error_code::AGENTSFLEET_NAME_EXISTS);
    assert_eq!(f.counts().await, (1, 1, 1));
    f.cleanup().await;
}
