//! The registered read against each writer whose rows it share-locks.
//!
//! Every writer here takes its first lock and then queues on a GATE session
//! holding a share lock on the row it touches second. The read starts while the
//! writer waits, and the gate lets go only once the read is queued too. A read
//! that took the writer's second row before its first now holds what the writer
//! needs while waiting on what the writer holds: Postgres answers one of them
//! with a deadlock (`40P01`). A read that locks in the writers' order queues
//! holding nothing the writer wants, and both finish.
//!
//! | writer                     | first lock               | second lock              |
//! |----------------------------|--------------------------|--------------------------|
//! | admin catalogue delete     | `model.catalogue_version`| `model.platform_models`  |
//! | registry entry removal     | `vault.secrets`          | `model.tenant_models`    |
//! | credential delete          | `vault.secrets`          | `model.tenant_models`    |
//!
//! The waits poll `pg_stat_activity` for blocking pids, never a table any test
//! locks.

use std::time::Duration;

use afd_admin::{DeleteModel, Models};
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_credential::provider::{Added, Entry, Removed, registered_model};
use afd_crypto::entropy::Entropy;
use afd_db::test_util::{ModelSeed, mint_id};
use afd_vault::{Directory, SecretName};
use sqlx::pool::PoolConnection;
use sqlx::{Acquire as _, Postgres};

use super::registry::providers;
use super::{Fixture, NOW};

/// The credential the entry hangs off.
const CREDENTIAL: &str = "a-locked-provider-key";

/// A credential body; the read never opens it.
const BODY: &[u8] = br#"{"provider":"openai","api_key":"sk-live"}"#;

/// The provider the catalogue row and the credential agree on.
const PROVIDER: &str = "openai";

/// The catalogue context the seeded model carries.
const CONTEXT: i32 = 64_000;

/// Share-locks one catalogue row: the gate before an admin delete.
const SHARE_MODEL: &str = "SELECT 1 FROM model.platform_models WHERE id = $1::uuid FOR SHARE";

/// Share-locks one registry entry: the gate before a removal or a credential
/// delete.
const SHARE_ENTRY: &str = "SELECT 1 FROM model.tenant_models WHERE id = $1::uuid FOR SHARE";

/// Whether any session waits behind `$1`.
const BLOCKS: &str =
    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1::int = ANY(pg_blocking_pids(pid)))";

/// Whether `$1` waits on anyone.
const WAITING: &str = "SELECT cardinality(pg_blocking_pids($1::int)) > 0";

/// Interval between looks at `pg_stat_activity`.
const POLL: Duration = Duration::from_millis(10);

/// Ten seconds at [`POLL`], past any queueing a loaded lane takes.
const POLLS: u32 = 1_000;

/// One seeded entry over a priced catalogue row, and the row's id.
struct Seeded {
    entry: Entry,
    model_id: String,
}

async fn seeded(fixture: &Fixture) -> Seeded {
    fixture
        .seed_with_shape(CREDENTIAL, BODY, Some(PROVIDER), Some(true))
        .await;
    let model_id = mint_id();
    let model = format!("gpt-locks-{model_id}");
    ModelSeed::new(&model_id, PROVIDER, &model, CONTEXT)
        .verified_at(NOW)
        .insert(&mut *fixture.database.acquire().await.expect("a connection"))
        .await
        .expect("the catalogue row seeds");
    let Added::Stored(entry) = providers(fixture)
        .add_entry(&fixture.tenant, &model, CREDENTIAL, NOW)
        .await
        .expect("an entry over a stored credential is an outcome")
    else {
        panic!("the credential is stored and the model is new, so the entry stores");
    };
    Seeded { entry, model_id }
}

/// A pooled session and its backend pid.
async fn session(fixture: &Fixture) -> (PoolConnection<Postgres>, i32) {
    let mut connection = fixture.database.acquire().await.expect("a connection");
    let pid = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *connection)
        .await
        .expect("the session names its backend");
    (connection, pid)
}

/// Polls `statement` bound to `pid` until it answers true.
async fn until(fixture: &Fixture, statement: &'static str, pid: i32, what: &str) {
    for _ in 0..POLLS {
        let mut probe = fixture
            .database
            .acquire()
            .await
            .expect("a probe connection");
        let met: bool = sqlx::query_scalar(statement)
            .bind(pid)
            .fetch_one(&mut *probe)
            .await
            .expect("pg_stat_activity answers");
        if met {
            return;
        }
        tokio::time::sleep(POLL).await;
    }
    panic!("{what} never happened for backend {pid}");
}

/// The writer is queued on the gate.
async fn writer_waits_on(fixture: &Fixture, gate: i32) {
    until(fixture, BLOCKS, gate, "a writer queueing on the gate").await;
}

/// Runs the registered read for `entry` on its own transaction, once the
/// writer is queued, and answers whether a routable row came back.
async fn read_after(
    fixture: &Fixture,
    reader: &mut PoolConnection<Postgres>,
    gate: i32,
    entry: &Uuid7,
) -> afd_credential::Result<bool> {
    writer_waits_on(fixture, gate).await;
    let mut transaction = reader.begin().await.expect("the read's transaction opens");
    let found = registered_model(&mut transaction, &fixture.tenant, entry.as_str())
        .await?
        .is_some();
    transaction.commit().await.expect("the read commits");
    Ok(found)
}

/// Releases the gate once the read is queued.
async fn release_after(fixture: &Fixture, gate: sqlx::Transaction<'_, Postgres>, reader: i32) {
    until(fixture, WAITING, reader, "the read queueing").await;
    gate.commit().await.expect("the gate releases");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_catalogue_delete_and_a_registered_read_both_finish() {
    let fixture = Fixture::create().await;
    let seeded = seeded(&fixture).await;
    let model = Uuid7::parse(&seeded.model_id).expect("the minted model id is UUIDv7");
    let models = Models::new(fixture.database.clone(), Entropy::new());
    let (mut gate_session, gate_pid) = session(&fixture).await;
    let mut gate = gate_session.begin().await.expect("the gate opens");
    sqlx::query(SHARE_MODEL)
        .bind(&seeded.model_id)
        .fetch_one(&mut *gate)
        .await
        .expect("the gate holds the catalogue row");
    let (mut reader, reader_pid) = session(&fixture).await;

    let (deleted, read, ()) = tokio::join!(
        models.delete(&model),
        read_after(&fixture, &mut reader, gate_pid, &seeded.entry.id),
        release_after(&fixture, gate, reader_pid),
    );

    assert_eq!(
        deleted.expect("the catalogue delete is not a deadlock victim"),
        DeleteModel::Deleted
    );
    assert!(
        !read.expect("the read is not a deadlock victim"),
        "the read ran after the delete committed, so the model is gone"
    );
    drop(reader);
    drop(gate_session);
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_entry_removal_and_a_registered_read_both_finish() {
    let fixture = Fixture::create().await;
    let seeded = seeded(&fixture).await;
    let store = providers(&fixture);
    let (mut gate_session, gate_pid) = session(&fixture).await;
    let mut gate = gate_session.begin().await.expect("the gate opens");
    sqlx::query(SHARE_ENTRY)
        .bind(seeded.entry.id.as_str())
        .fetch_one(&mut *gate)
        .await
        .expect("the gate holds the entry");
    let (mut reader, reader_pid) = session(&fixture).await;

    let (removed, read, ()) = tokio::join!(
        store.remove_entry(&fixture.tenant, &seeded.entry.id),
        read_after(&fixture, &mut reader, gate_pid, &seeded.entry.id),
        release_after(&fixture, gate, reader_pid),
    );

    assert_eq!(
        removed.expect("the removal is not a deadlock victim"),
        Removed::Done
    );
    assert!(
        !read.expect("the read is not a deadlock victim"),
        "the read ran after the removal committed, so the entry is gone"
    );
    drop(reader);
    drop(gate_session);
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_credential_delete_and_a_registered_read_both_finish() {
    let fixture = Fixture::create().await;
    let seeded = seeded(&fixture).await;
    let directory = Directory::new(fixture.database.clone());
    let name = SecretName::parse(CREDENTIAL).expect("the fixture name is valid");
    let (mut gate_session, gate_pid) = session(&fixture).await;
    let mut gate = gate_session.begin().await.expect("the gate opens");
    sqlx::query(SHARE_ENTRY)
        .bind(seeded.entry.id.as_str())
        .fetch_one(&mut *gate)
        .await
        .expect("the gate holds the entry");
    let (mut reader, reader_pid) = session(&fixture).await;

    let (deleted, read, ()) = tokio::join!(
        directory.delete(&fixture.workspace, &name),
        read_after(&fixture, &mut reader, gate_pid, &seeded.entry.id),
        release_after(&fixture, gate, reader_pid),
    );

    let refused = deleted.expect_err("the entry still names the credential");
    assert_eq!(
        refused.code(),
        error_code::SECRET_REFERENCED_BY_MODEL_ENTRIES,
        "the delete refuses on the reference count, not a deadlock: {refused}"
    );
    assert!(
        read.expect("the read is not a deadlock victim"),
        "the refused delete left the entry and its credential routable"
    );
    drop(reader);
    drop(gate_session);
    fixture.cleanup().await;
}
