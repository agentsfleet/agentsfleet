//! Runnable sibling fleets share their scenario's labels, pricing and capacity owner.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use crate::{
    e2e::{ACTOR, EVENT_TYPE, REQUEST_JSON, Scenario},
    e2e_native::Stored,
    e2e_seed,
};
use af_core::{clock, id::Uuid7};
use afd_db::test_util::{FleetSeed, mint_id};
use afd_fleet_lifecycle::FleetStatus;
use afd_turns::{Admission, Admissions, Key, Producer, Reply};

pub(super) async fn sibling(scene: &Scenario, name: &str) -> (Uuid7, Uuid7) {
    let fleet = seed_sibling(scene, name).await;
    let admitted = Admissions::for_tests(scene.booted.database.clone(), scene.booted.bus.clone())
        .admit(Admission {
            producer: Producer::Steer,
            key: Key::Unrepeatable,
            fleet: &fleet,
            workspace: &scene.workspace,
            actor: ACTOR,
            event_type: EVENT_TYPE,
            request_json: REQUEST_JSON,
            reply: Reply::None,
        })
        .await
        .expect("admit sibling event");
    (fleet, admitted.stored.id)
}

async fn seed_sibling(scene: &Scenario, name: &str) -> Uuid7 {
    let fleet = mint_id();
    let stored = Stored::of(&crate::e2e_native::definition(
        e2e_seed::FLEET_CONFIG_JSON,
        e2e_seed::FIXTURE_INSTRUCTIONS,
    ));
    let tag = e2e_seed::scenario_tag(&scene.fleet);
    let mut connection = scene
        .booted
        .database
        .acquire()
        .await
        .expect("sibling fixture connection");
    FleetSeed::new(
        &fleet,
        &scene.workspace,
        &scene.tenant,
        FleetStatus::Active.as_str(),
    )
    .name(name)
    .tags(&[tag.as_str()])
    .at(clock::now())
    .setup(stored.revision())
    .insert(&mut *connection)
    .await
    .expect("second runnable fleet");
    sqlx::query("UPDATE runner.runners SET worker_count=$2 WHERE id=$1")
        .bind(&scene.runner_id)
        .bind(2_i32)
        .execute(&mut *connection)
        .await
        .expect("two physical worker slots");
    drop(connection);
    fleet
}
