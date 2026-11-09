//! Runnable sibling fleets share their scenario's labels, pricing and capacity owner.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use crate::{
    e2e::{ACTOR, EVENT_TYPE, REQUEST_JSON, Scenario},
    e2e_native::Stored,
    e2e_seed,
};
use af_core::{clock, id::Uuid7};
use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_db::test_util::{FleetSeed, mint_id};
use afd_fleet_lifecycle::FleetStatus;

pub(super) async fn sibling(run: &Scenario, name: &str) -> (Uuid7, Uuid7) {
    let fleet = seed_sibling(run, name).await;
    let admitted = Admissions::for_tests(run.booted.database.clone(), run.booted.bus.clone())
        .admit(Admission {
            producer: Producer::Steer,
            key: Key::Unrepeatable,
            fleet: &fleet,
            workspace: &run.workspace,
            actor: ACTOR,
            event_type: EVENT_TYPE,
            request_json: REQUEST_JSON,
            reply: Reply::None,
        })
        .await
        .expect("admit sibling event");
    (fleet, admitted.stored.id)
}

async fn seed_sibling(run: &Scenario, name: &str) -> Uuid7 {
    let fleet = mint_id();
    let stored = Stored::of(&crate::e2e_native::definition(
        e2e_seed::FLEET_CONFIG_JSON,
        e2e_seed::FIXTURE_INSTRUCTIONS,
    ));
    let tag = e2e_seed::scenario_tag(&run.fleet);
    let mut connection = run
        .booted
        .database
        .acquire()
        .await
        .expect("sibling fixture connection");
    FleetSeed::new(
        &fleet,
        &run.workspace,
        &run.tenant,
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
        .bind(&run.runner_id)
        .bind(2_i32)
        .execute(&mut *connection)
        .await
        .expect("two physical worker slots");
    drop(connection);
    fleet
}
