//! Every protected verb rechecks its connection after a real host-row lock wait.

use super::{
    Checked, lifecycle::isolated, manifest::Pending, publication::command, storage::Files, valid,
};
use af_core::{clock, error_code, id::Uuid7};
use af_crypto::entropy::Entropy;
use af_wire::run::{
    CheckpointOperationId, TurnCheckpointAction as Action, TurnCheckpointRef,
    TurnCheckpointResponse as Response, TurnWorkspace,
};
use afd_auth::{credential::CredentialKind, minted::Minted};
use afd_db::test_util::mint_id;
use afd_runner::{Runners, capacity::authority::Decision};
use sqlx::{Connection as _, PgConnection};
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
enum Operation {
    Begin,
    Commit,
    Read,
    Adopt,
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn every_checkpoint_verb_refuses_a_connection_superseded_while_waiting() -> Checked {
    for operation in [
        Operation::Begin,
        Operation::Commit,
        Operation::Read,
        Operation::Adopt,
    ] {
        refused_after_wait(operation, false).await?;
    }
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn every_checkpoint_verb_refuses_a_credential_rotated_while_waiting() -> Checked {
    for operation in [
        Operation::Begin,
        Operation::Commit,
        Operation::Read,
        Operation::Adopt,
    ] {
        refused_after_wait(operation, true).await?;
    }
    Ok(())
}

async fn refused_after_wait(operation: Operation, rotates: bool) -> Checked {
    let files = isolated().await?;
    let request = valid(command(&files.case, prepare(&files, operation).await?))?;
    let bound = connected(&files).await?;
    // Prove this exact request succeeds with live authority, including replay.
    files
        .store
        .execute(&bound, files.case.run(), &request)
        .await?;
    let before = witness(&files).await?;
    let result = blocked(&files, &bound, &request, rotates).await?;
    let Err(failure) = result else {
        return Err(format!("{operation:?} retained superseded authority").into());
    };
    assert_eq!(failure.code(), error_code::RUN_LOST, "{operation:?}");
    assert_eq!(witness(&files).await?, before, "{operation:?}");
    files.finish().await;
    Ok(())
}

async fn connected(files: &Files) -> Checked<afd_fleet::run::Runs> {
    let runners = Runners::new(files.case.fixtures.database.clone(), Entropy::new());
    let token = runners
        .platform()
        .rotate_token(&files.case.runner, "custody-test", clock::now())
        .await?;
    let Decision::Allowed(owner) = runners
        .acquire_connection(
            &files.case.runner,
            token.digest().clone(),
            None,
            clock::now(),
        )
        .await?
    else {
        return Err("fresh checkpoint owner refused".into());
    };
    Ok(files.case.runs.clone().for_connection(owner))
}

async fn blocked(
    files: &Files,
    bound: &afd_fleet::run::Runs,
    request: &garde::Valid<af_wire::run::TurnCheckpointCommand>,
    rotates: bool,
) -> Checked<afd_fleet::Result<Response>> {
    let mut connection = files.case.fixtures.database.acquire().await?;
    let mut held = connection.begin().await?;
    let blocker: i32 =
        sqlx::query_scalar("SELECT pg_backend_pid() FROM runner.runners WHERE id=$1 FOR UPDATE")
            .bind(&files.case.runner)
            .fetch_one(&mut *held)
            .await?;
    let replace = async {
        let mut observer = files.case.fixtures.database.acquire().await?;
        wait_behind(&mut observer, blocker).await?;
        replace_owner(&mut held, &files.case.runner, rotates).await?;
        held.commit().await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    };
    let (result, replacement) = tokio::join!(
        files.store.execute(bound, files.case.run(), request),
        replace,
    );
    replacement?;
    drop(connection);
    Ok(result)
}

async fn prepare(files: &Files, operation: Operation) -> Checked<Action> {
    let pending = Pending::allocate(files, &[], TurnWorkspace::ApiOnly, None).await?;
    if matches!(operation, Operation::Commit) {
        return Ok(Action::Commit {
            checkpoint: pending.reference,
            root: pending.root,
            manifest_root: af_crypto::checkpoint_digest::manifest_root([]),
            folds: Vec::new(),
        });
    }
    let Response::Committed(saved) = pending.commit(files, &[]).await? else {
        return Err("committed checkpoint".into());
    };
    match operation {
        Operation::Begin => {
            let mut request = files.case.command();
            let Action::Begin(begin) = &mut request.action else {
                return Err("begin request".into());
            };
            begin.expected_head = Some(saved.checkpoint_id.clone());
            Ok(request.action)
        }
        Operation::Read => Ok(Action::Read { descriptor: saved }),
        Operation::Adopt => {
            let Response::Read(read) = files.execute(Action::Read { descriptor: saved }).await?
            else {
                return Err("reader custody".into());
            };
            Ok(Action::Adopt {
                checkpoint: TurnCheckpointRef {
                    checkpoint_id: read.descriptor.checkpoint_id,
                    pin_id: CheckpointOperationId(read.pin.id),
                },
            })
        }
        Operation::Commit => unreachable!("prepared before publication"),
    }
}

async fn witness(files: &Files) -> Checked<(serde_json::Value, serde_json::Value)> {
    Ok(sqlx::query_as(
        "SELECT
        jsonb_build_object(
          'head', (SELECT to_jsonb(r) FROM runner.turn_recovery r WHERE turn_id=$1),
          'plans', (SELECT jsonb_agg(jsonb_build_array(id,ready_at,abandoned_at,generation) ORDER BY id)
                    FROM runner.turn_checkpoints WHERE turn_id=$1),
          'pins', (SELECT jsonb_agg(jsonb_build_array(p.id,p.resource_version) ORDER BY p.id)
                   FROM runner.turn_checkpoint_pins p JOIN runner.turn_checkpoints c
                   ON c.id=p.turn_checkpoint_id WHERE c.turn_id=$1)),
        (SELECT jsonb_agg(jsonb_build_array(id,run_fence,input_claim_run_id,input_claim_fence) ORDER BY id)
         FROM thread.turns WHERE thread_id=$2)",
    ).bind(&files.case.acquired.turn_id).bind(&files.case.acquired.fleet_id)
      .fetch_one(&mut *files.case.fixtures.database.acquire().await?).await?)
}

async fn replace_owner(connection: &mut PgConnection, runner: &Uuid7, rotates: bool) -> Checked {
    if rotates {
        let token = Minted::draw(CredentialKind::RunnerToken, &Entropy::new())?;
        sqlx::query("UPDATE runner.runners SET token_hash=$2,connection_id=NULL WHERE id=$1")
            .bind(runner)
            .bind(token.digest().as_str())
            .execute(connection)
            .await?;
    } else {
        sqlx::query("UPDATE runner.runners SET connection_id=$2 WHERE id=$1")
            .bind(runner)
            .bind(mint_id())
            .execute(connection)
            .await?;
    }
    Ok(())
}

async fn wait_behind(connection: &mut PgConnection, blocker: i32) -> Checked {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            ).bind(blocker).fetch_one(&mut *connection).await?;
            if waiting {
                return Ok::<_, sqlx::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await??;
    Ok(())
}
