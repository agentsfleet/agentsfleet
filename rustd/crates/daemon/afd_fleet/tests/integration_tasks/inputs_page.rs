//! A run's input page carries its source message only when a person wrote it.

use af_wire::task::TaskInputsQuery;
use afd_fleet::task::TaskLifecycle as _;

use super::fixture::{Case, GOALS, exercise};

fn first_page(case: &Case) -> garde::Valid<TaskInputsQuery> {
    garde::Unvalidated::new(TaskInputsQuery {
        fencing_token: case.run().fencing_token,
        starting_after: None,
        after_input_sequence: None,
        limit: 1,
    })
    .validate()
    .expect("bounded input page")
}

// Value: protects=a run started by a machine actor is never shown a source
// message as if a person had asked for it, while a person's run is;
// fails_when=the human check is dropped from the current-message read, or it
// hides a person's message; why_new=the non-human arm of the current message had
// no live case; seam=the production input page over live Postgres, the run's
// source turn re-attributed to the machine actor.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_machine_started_run_reads_no_current_message() {
    exercise(async |case| {
        let human = case
            .tasks
            .inputs(case.run(), &first_page(case))
            .await
            .expect("a person's run reads its page");
        assert_eq!(
            human.current_message.as_deref(),
            Some(GOALS.join("; ").as_str())
        );

        let moved = sqlx::query("UPDATE thread.turns SET actor=$2 WHERE id=$1")
            .bind(&case.held.turn_id)
            .bind(afd_turns::view::ACTOR_MACHINE)
            .execute(
                &mut *case
                    .held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("actor connection"),
            )
            .await
            .expect("the source turn is the machine's");
        assert_eq!(moved.rows_affected(), 1);

        let machine = case
            .tasks
            .inputs(case.run(), &first_page(case))
            .await
            .expect("a machine's run still reads its page");
        assert_eq!(machine.current_message, None);
        assert_eq!(machine.source_turn_id, case.held.turn_id);
        assert!(machine.inputs.is_empty(), "no person's input is pending");
    })
    .await;
}
