//! Old event keys are removed only after every object and uncompleted purge ends.

use super::{Case, Checked, files, storage::Files};
use crate::support::Fixtures;
use af_core::id::Uuid7;
use af_crypto::{aad::Aad, envelope::Sealer, secret::Kek};
use af_wire::lease::{TurnCheckpointRef, TurnId};
use afd_fleet::task::{CheckpointMaintenanceCursor, TaskLifecycle as _};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn event_identity_retention_is_bounded_fair_and_waits_for_object_and_purge_custody() -> Checked
{
    let case = Case::with_fixtures(Fixtures::create_isolated().await).await?;
    let files = Files::for_case(case).await?;
    let keys = seed(&files).await?;
    let tasks = files.case.tasks();
    let mut cursor = CheckpointMaintenanceCursor::default();
    let first = tasks.maintain_checkpoints(&mut cursor).await?;
    assert_eq!(
        first.identity_keys, 0,
        "the first bounded page contains repository keys"
    );
    let mut removed = 0;
    for _ in 0..3 {
        removed += tasks.maintain_checkpoints(&mut cursor).await?.identity_keys;
    }
    assert_eq!(
        removed, 2,
        "unused old event keys and completed purges can end"
    );
    finish_purge_after_asserting_retention(&files, &keys).await?;
    let mut collected = 0;
    for _ in 0..3 {
        collected += tasks.maintain_checkpoints(&mut cursor).await?.identity_keys;
    }
    assert_eq!(
        collected, 1,
        "acknowledged purge custody permits the remaining event key to end"
    );
    files.finish().await;
    Ok(())
}

async fn finish_purge_after_asserting_retention(files: &Files, keys: &Keys) -> Checked {
    let mut connection = files.case.fixtures.database.acquire().await?;
    let retained: Vec<Uuid7> = sqlx::query_scalar(
        "SELECT id FROM fleet.task_identity_keys WHERE workspace_id=$1 ORDER BY id",
    )
    .bind(&files.case.acquired.workspace_id)
    .fetch_all(&mut *connection)
    .await?;
    let expected: std::collections::BTreeSet<_> = keys
        .all
        .iter()
        .filter(|id| !keys.removed.contains(id))
        .cloned()
        .collect();
    assert_eq!(
        retained
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        expected
    );
    sqlx::query("UPDATE fleet.task_storage_purges SET completed_at=now(),updated_at=now() WHERE task_identity_key_id=$1")
        .bind(&keys.blocked).execute(&mut *connection).await?;
    drop(connection);
    Ok(())
}

struct Keys {
    all: Vec<Uuid7>,
    removed: Vec<Uuid7>,
    blocked: Uuid7,
}

async fn seed(files: &Files) -> Checked<Keys> {
    let used = used_identity(files).await?;
    let mut connection = files.case.fixtures.database.acquire().await?;
    let mut keys = Keys {
        all: vec![used],
        removed: Vec::new(),
        blocked: afd_db::test_util::mint_id(),
    };
    keys.all
        .extend(repository_keys(&mut connection, &files.case.acquired.workspace_id).await?);
    for (age, repository, custody) in [
        (10, namespace(), None),
        (1, namespace(), None),
        (10, "owner/retained".to_owned(), None),
        (10, namespace(), Some(false)),
        (10, namespace(), Some(true)),
    ] {
        let id = insert(
            &mut connection,
            &files.case.acquired.workspace_id,
            &repository,
            age,
        )
        .await?;
        if let Some(done) = custody {
            purge(
                &mut connection,
                &files.case.acquired.workspace_id,
                &id,
                done,
            )
            .await?;
            if !done {
                keys.blocked = id.clone();
            }
        }
        if age == 10 && repository.starts_with("turn:") && custody != Some(false) {
            keys.removed.push(id.clone());
        }
        keys.all.push(id);
    }
    Ok(keys)
}

async fn repository_keys(
    connection: &mut sqlx::PgConnection,
    workspace: &Uuid7,
) -> Checked<Vec<Uuid7>> {
    let mut keys = Vec::new();
    for index in 0..101 {
        keys.push(
            insert(
                connection,
                workspace,
                &format!("owner/repository-{index}"),
                12,
            )
            .await?,
        );
    }
    Ok(keys)
}

async fn used_identity(files: &Files) -> Checked<Uuid7> {
    let (plan, _) = files::begin(files).await?;
    files::authenticated_chunk(
        files,
        &TurnCheckpointRef {
            checkpoint_id: plan.checkpoint_id,
            pin_id: TurnId(plan.pin.id),
        },
    )
    .await?;
    let mut connection = files.case.fixtures.database.acquire().await?;
    let used: Uuid7 = sqlx::query_scalar(
        "UPDATE fleet.task_identity_keys SET created_at=clock_timestamp()-interval '10 days' WHERE workspace_id=$1 RETURNING id",
    ).bind(&files.case.acquired.workspace_id).fetch_one(&mut *connection).await?;
    Ok(used)
}

fn namespace() -> String {
    format!("turn:{}", afd_db::test_util::mint_id())
}

async fn insert(
    connection: &mut sqlx::PgConnection,
    workspace: &Uuid7,
    repository: &str,
    age: i32,
) -> Checked<Uuid7> {
    let id = afd_db::test_util::mint_id();
    let master = Kek::from_hex(&hex::encode([17_u8; 32]))?;
    let envelope = Sealer::new().seal(
        &master,
        &Aad::checkpoint_identity(workspace, &id, repository, 1, af_crypto::KEK_VERSION),
        &[11_u8; 32],
    )?;
    sqlx::query("INSERT INTO fleet.task_identity_keys
        (id,workspace_id,repository,key_version,dek_ciphertext,dek_nonce,dek_tag,secret_nonce,secret_ciphertext,secret_tag,kek_version,created_at,updated_at)
        VALUES ($1,$2,$3,1,$4,$5,$6,$7,$8,$9,$10,now()-make_interval(days=>$11),now())")
        .bind(&id).bind(workspace).bind(repository).bind(envelope.wrapped_dek())
        .bind(envelope.dek_nonce().as_slice()).bind(envelope.dek_tag().as_slice())
        .bind(envelope.payload_nonce().as_slice()).bind(envelope.payload_ciphertext())
        .bind(envelope.payload_tag().as_slice()).bind(envelope.kek_version()).bind(age)
        .execute(connection).await?;
    Ok(id)
}

async fn purge(
    connection: &mut sqlx::PgConnection,
    workspace: &Uuid7,
    identity: &Uuid7,
    done: bool,
) -> Checked {
    let object = afd_db::test_util::mint_id();
    sqlx::query("INSERT INTO fleet.task_storage_purges
        (task_object_id,workspace_id,task_identity_key_id,bucket_reference,accounted_bytes,stored,delete_after_at,next_attempt_at,completed_at,created_at,updated_at)
        VALUES ($1,$2,$3,$4,0,false,now()+interval '1 day',now()+interval '1 day',CASE WHEN $5 THEN now() ELSE NULL END,now(),now())")
        .bind(&object).bind(workspace).bind(identity).bind(object.to_string()).bind(done)
        .execute(connection).await?;
    Ok(())
}
