//! A question's answer runs on the checkpoint of the turn that first asked,
//! however many answers on the way asked again.

use af_core::id::Uuid7;
use af_wire::run::TurnWorkspace;
use af_wire::turn::TurnKind;
use afd_db::test_util::mint_id;
use afd_turns::Producer;

use super::{
    Case, Checked,
    publication::{publish, root},
};

/// Moves `case`'s run onto a new answer resuming `asked`, as an answered
/// question's continuation is written, and returns the answer.
async fn answer(case: &Case, asked: &Uuid7) -> Checked<Uuid7> {
    let answer = mint_id();
    let mut connection = case.fixtures.database.acquire().await?;
    sqlx::query(
        "INSERT INTO thread.turns (id, thread_id, workspace_id, kind, actor, status, producer, \
           producer_key, payload_digest, request_json, resumes_turn_id, run_fence, created_at, \
           updated_at) \
         SELECT $1, thread_id, workspace_id, $2, actor, status, $3, $4, payload_digest, \
           request_json, id, run_fence, created_at, updated_at \
         FROM thread.turns WHERE id = $5",
    )
    .bind(&answer)
    .bind(TurnKind::Continuation.as_str())
    .bind(Producer::QuestionAnswer.as_str())
    .bind(mint_id().as_str().to_owned())
    .bind(asked)
    .execute(&mut *connection)
    .await?;
    sqlx::query("UPDATE thread.runs SET turn_id = $2 WHERE id = $1")
        .bind(&case.issued.run_id)
        .bind(&answer)
        .execute(&mut *connection)
        .await?;
    Ok(answer)
}

// Value: protects=an answer to a question its answer's run asked restores what
// that run saved; fails_when=the owner read stops at the turn the answer
// resumes; why_new=every answer case resumed a turn that asked first;
// seam=TurnCheckpoints::resume over live Postgres, the run moved onto each answer.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_second_answer_restores_what_the_first_answer_saved() -> Checked {
    let case = Case::new().await?;
    let store = super::turn_store(&case)?;
    let first = answer(&case, &case.acquired.turn_id).await?;
    let saved = publish(&case, &store, &root(&case, TurnWorkspace::ApiOnly)).await?;
    answer(&case, &first).await?;

    assert_eq!(
        super::turn_store(&case)?
            .resume(&case.runs, case.run())
            .await?,
        Some(*saved),
        "the second answer resumes the head the first answer's run saved"
    );
    case.fixtures.cleanup().await;
    Ok(())
}

// Value: protects=a resume cycle cannot hang or misdirect the owner read;
// fails_when=the walk over resumes_turn_id has no cycle guard; why_new=every
// answer case walked an acyclic chain; seam=TurnCheckpoints::resume over live
// Postgres, two answers resuming each other.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn answers_that_resume_each_other_end_the_walk() -> Checked {
    let case = Case::new().await?;
    let store = super::turn_store(&case)?;
    let first = answer(&case, &case.acquired.turn_id).await?;
    publish(&case, &store, &root(&case, TurnWorkspace::ApiOnly)).await?;
    let second = answer(&case, &first).await?;
    sqlx::query("UPDATE thread.turns SET resumes_turn_id = $2 WHERE id = $1")
        .bind(&first)
        .bind(&second)
        .execute(&mut *case.fixtures.database.acquire().await?)
        .await?;

    assert_eq!(
        super::turn_store(&case)?
            .resume(&case.runs, case.run())
            .await?,
        None,
        "the cycle ends on the first answer, which saved nothing"
    );
    case.fixtures.cleanup().await;
    Ok(())
}
