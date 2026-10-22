//! Transport delay amplifies round trips without changing the pool's budget.

use super::{Fixture, SendMessage, claim, fund, mint, now, queue, unknown};
use afd_authoring::{Drafts, Result};
use afd_crypto::entropy::Entropy;
use afd_db::{Db, config::DbRole, test_util::TestDatabase};
use reqwest::Url;
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
    task::{JoinHandle, JoinSet},
};

const DELAY: Duration = Duration::from_millis(10);

struct DelayedDatabase {
    url: String,
    task: JoinHandle<()>,
}

impl DelayedDatabase {
    async fn start(owner: &str) -> Self {
        let target = Url::parse(owner).expect("fixture owner URL");
        let upstream = (
            target.host_str().expect("database host").to_owned(),
            target.port().unwrap_or(5432),
        );
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("proxy listener");
        let address = listener.local_addr().expect("bound proxy address");
        let mut url = Url::parse(&TestDatabase::shared().api_url()).expect("fixture API URL");
        url.set_path(target.path());
        url.set_host(Some("127.0.0.1")).expect("loopback host");
        url.set_port(Some(address.port()))
            .expect("bound proxy port");
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (client, _) = accepted.expect("accept proxy connection");
                        let upstream = upstream.clone();
                        connections.spawn(async move {
                            let server = TcpStream::connect(upstream).await.expect("real Postgres");
                            let (client_read, client_write) = client.into_split();
                            let (server_read, server_write) = server.into_split();
                            // A database client may close either half first.
                            let _closed = tokio::try_join!(relay(client_read, server_write), relay(server_read, client_write));
                        });
                    }
                    result = connections.join_next(), if !connections.is_empty() => {
                        result.expect("proxy connection").expect("proxy task");
                    }
                }
            }
        });
        Self {
            url: url.into(),
            task,
        }
    }

    async fn open(&self) -> Db {
        TestDatabase::shared()
            .open(DbRole::Api, &[("DATABASE_URL_API", &self.url)])
            .await
    }
}

impl Drop for DelayedDatabase {
    fn drop(&mut self) {
        // Dropping the supervisor also drops its JoinSet and every live socket.
        self.task.abort();
    }
}

async fn relay<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut read: R,
    mut write: W,
) -> Result<(), std::io::Error> {
    let mut buffer = vec![0; 65_536];
    loop {
        let size = read.read(&mut buffer).await?;
        if size == 0 {
            return write.shutdown().await;
        }
        tokio::time::sleep(DELAY).await;
        write
            .write_all(buffer.get(..size).expect("read count fits its buffer"))
            .await?;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live Postgres"]
async fn delayed_database_keeps_one_hundred_sends_within_the_existing_pool_budget() {
    let mut f = fund(Fixture::isolated().await, 2).await;
    let draft = queue(&f, "Original unknown request").await;
    let lease = claim(&f, &draft).await.expect("first attempt");
    unknown(&f, &lease).await;
    f.drafts
        .cancel(f.access(), &draft.id, 1)
        .await
        .expect("Stop old work");
    super::history(&f, &draft, &lease).await;
    let proxy = DelayedDatabase::start(&f.owner_url()).await;
    let database = proxy.open().await;
    f.drafts = Drafts::new(
        database.clone(),
        Entropy::new(),
        crate::draft::fixture::ledger(&f.db),
    );
    let peer = Drafts::new(
        database.clone(),
        Entropy::new(),
        crate::draft::fixture::ledger(&f.db),
    );
    let fixture = Arc::new(f);
    super::sends(Arc::clone(&fixture), &draft.id, peer).await;
    assert_eq!(stored(&fixture, &draft.id).await, (101, 101, 1_100));
    Arc::try_unwrap(fixture)
        .ok()
        .expect("all senders released")
        .cleanup()
        .await;
    database.close().await;
    drop(proxy);
}

async fn stored(f: &Fixture, draft: &afd_core::id::Uuid7) -> (i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM authoring.messages WHERE draft_id=d.id), (SELECT count(*) FROM authoring.replay_receipts WHERE scope=$2), d.last_message_sequence FROM authoring.drafts d WHERE d.id=$1::uuid")
        .bind(draft.as_str())
        .bind(format!("{}:draft:{draft}:message", f.workspace))
        .fetch_one(&mut *f.connection().await)
        .await.expect("committed message, receipt and sequence")
}

async fn version(f: &Fixture, draft: &afd_core::id::Uuid7, value: i64) {
    sqlx::query("UPDATE authoring.drafts SET resource_version=$2 WHERE id=$1::uuid")
        .bind(draft.as_str())
        .bind(value)
        .execute(&mut *f.connection().await)
        .await
        .expect("owned version fault");
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn failed_sequence_update_rolls_back_message_and_receipt_then_allows_same_key_retry() {
    let f = Fixture::create().await;
    let draft = queue(&f, "Original request").await;
    let operation = mint();
    let request = || SendMessage {
        message: "Next request".into(),
        operation_id: operation.clone(),
    };
    let before = stored(&f, &draft.id).await;
    version(&f, &draft.id, i64::MAX).await;
    let error = f
        .drafts
        .send(f.access(), &draft.id, request(), now())
        .await
        .expect_err("version overflow");
    assert_eq!(error.code(), afd_core::error_code::INTERNAL_DB_QUERY);
    let cause = std::error::Error::source(&error)
        .and_then(|source| source.downcast_ref::<sqlx::Error>())
        .and_then(sqlx::Error::as_database_error)
        .expect("original database cause");
    assert_eq!(cause.code().as_deref(), Some("22003"));
    assert_eq!(stored(&f, &draft.id).await, before);
    version(&f, &draft.id, 2).await;
    let accepted = f
        .drafts
        .send(f.access(), &draft.id, request(), now())
        .await
        .expect("same key after rollback");
    assert_eq!(accepted.sequence, "2");
    assert!(!accepted.replayed);
    let replay = f
        .drafts
        .send(f.access(), &draft.id, request(), now())
        .await
        .expect("exact accepted replay");
    assert_eq!(replay.sequence, "2");
    assert!(replay.replayed);
    assert_eq!(stored(&f, &draft.id).await, (2, 2, 2));
    f.cleanup().await;
}
