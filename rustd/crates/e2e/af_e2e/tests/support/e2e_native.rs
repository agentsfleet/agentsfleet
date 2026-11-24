//! The model a seeded fleet runs on, changed the way a person's `PATCH`
//! changes it: the fleet row is the one place a run reads it from.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;

/// Points `fleet` at the model `model_ref` names and moves its resource tag,
/// as a model change does.
const SELECT_MODEL: &str = "\
UPDATE fleet.fleets SET model_ref = $2, resource_version = resource_version + 1,
    updated_at = now()
WHERE id = $1";

/// Makes `model_ref` the model `fleet`'s next run resolves.
pub(crate) async fn select_model(
    connection: &mut sqlx::PgConnection,
    fleet: &Uuid7,
    model_ref: &str,
) {
    sqlx::query(SELECT_MODEL)
        .bind(fleet)
        .bind(model_ref)
        .execute(connection)
        .await
        .expect("the fleet's model changes");
}
