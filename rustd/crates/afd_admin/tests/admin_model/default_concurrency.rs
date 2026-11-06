//! Competing first activations take the same parent lock before any default row.
use super::{Lane, NOW};
use afd_admin::{CreateModel, SetPlatformKey};
use sqlx::Acquire as _;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn first_activations_serialize_and_keep_one_visible_default() {
    super::isolated::run(async |lane| {
        lane.file_model().await;
        let provider = second_model(lane).await;
        let mut connection = lane.database.acquire().await.expect("lock connection");
        let mut transaction = connection.begin().await.expect("begin");
        let blocker: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *transaction)
            .await
            .expect("pid");
        sqlx::query("SELECT id FROM model.catalogue_version WHERE id = 1 FOR UPDATE")
            .execute(&mut *transaction)
            .await
            .expect("parent lock");
        let (first, second) = spawn_activations(lane, provider);
        await_waiters(lane, blocker, 2).await;
        assert_eq!(lane.keys.list().await.expect("metadata").len(), 0);
        transaction.commit().await.expect("release");
        drop(connection);
        assert!(matches!(
            first.await.expect("join").expect("first"),
            SetPlatformKey::Set(_)
        ));
        assert!(matches!(
            second.await.expect("join").expect("second"),
            SetPlatformKey::Set(_)
        ));
        verify_default(lane).await;
    })
    .await;
}

async fn await_waiters(lane: &Lane, blocker: i32, expected: i64) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let mut connection = lane.database.acquire().await.expect("observer");
            let waiters: i64 = sqlx::query_scalar(
                "WITH RECURSIVE waiting(pid) AS (
                    SELECT pid FROM pg_stat_activity
                    WHERE $1::integer = ANY(pg_blocking_pids(pid))
                    UNION
                    SELECT activity.pid FROM pg_stat_activity activity
                    JOIN waiting ON waiting.pid = ANY(pg_blocking_pids(activity.pid))
                 ) SELECT count(*) FROM waiting",
            )
            .bind(blocker)
            .fetch_one(&mut *connection)
            .await
            .expect("blocking graph");
            if waiters == expected {
                break;
            }
            drop(connection);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both activations block before writing metadata");
}

async fn second_model(lane: &Lane) -> String {
    let mut other = lane.input();
    other.provider.push_str("-other");
    let provider = other.provider.clone();
    assert!(matches!(
        lane.models.create(other, NOW).await.expect("second model"),
        CreateModel::Created(_)
    ));
    provider
}

async fn verify_default(lane: &Lane) {
    let defaults = lane.keys.list().await.expect("defaults");
    assert_eq!(defaults.len(), 2);
    assert_eq!(defaults.iter().filter(|row| row.active).count(), 1);
    let active = defaults.iter().find(|row| row.active).expect("one active");
    assert!(
        lane.keys
            .deactivate(&active.provider)
            .await
            .expect("deactivate")
    );
    assert!(
        lane.keys
            .list()
            .await
            .expect("metadata")
            .iter()
            .all(|row| !row.active)
    );
}

type Activation = tokio::task::JoinHandle<afd_admin::Result<SetPlatformKey>>;
fn spawn_activations(lane: &Lane, provider: String) -> (Activation, Activation) {
    let first_keys = lane.keys.clone();
    let first_input = lane.key_input();
    let first = tokio::spawn(async move { first_keys.set(first_input).await });
    let second_keys = lane.keys.clone();
    let mut second_input = lane.key_input();
    second_input.provider = provider;
    let second = tokio::spawn(async move { second_keys.set(second_input).await });
    (first, second)
}
