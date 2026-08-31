//! The one poster this build ships, against the two stores it actually holds.
//!
//! `dispatch` routes exactly one provider — `outbound/worker.zig:124` does the
//! same and says so — and `integration_worker.rs` drives it through a stub
//! `Deliver`, because what that suite grades is the consumer group and the
//! retry loop. So `SlackPoster` itself ran no covered lines: the pool read that
//! finds where an answer goes, the vault read that opens the bot token, and the
//! POST that carries it were all unproven.
//!
//! # Why a loopback Slack rather than a stubbed client
//!
//! The verdict a status maps to is already graded without a server in
//! `delivery.rs` — no server can make a vendor answer 429 three times on
//! demand. What only a socket can show is the REQUEST: that the bearer is the
//! token the vault opened, that the channel and thread come from the row the
//! mention ingress wrote, and that a 200 carrying `{"ok":false}` is not a
//! delivery. A stubbed client would assert the arguments this test passed it.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_connector::{Grants, Provider};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_outbound::{Deliver as _, Verdict};
use afd_redis::OutboundDelivery;
use afd_redis::streams::EventId;
use afd_vault::{SecretBody, SecretName, Vault};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;

/// How long the fake waits to be dialled before it gives up.
///
/// A deadline rather than an unbounded accept: a regression that answers
/// `Delivered` without connecting must FAIL the suite, not park it.
const ACCEPT_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

/// How long one request may take to arrive in full.
const READ_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

/// The key every fixture seals under — the harness's own, not a deployment's.
const FIXTURE_KEK: [u8; 32] = [7u8; 32];

/// The bot token the vault holds for this workspace.
const BOT_TOKEN: &str = "xoxb-fixture-bot-token";

/// Where the answer is threaded, as the mention ingress wrote it.
const CHANNEL: &str = "C0FIXTURE01";
/// See [`CHANNEL`].
const THREAD: &str = "1712345678.000100";

/// What the fleet is answering with.
const ANSWER: &str = "the fixture answer";

/// One loopback Slack, answering `body` with `status` to the first request.
///
/// Returns the base URL and a handle that yields what the daemon actually sent.
struct FakeSlack {
    base: String,
    request: tokio::task::JoinHandle<String>,
}

impl FakeSlack {
    async fn answering(status: &'static str, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port is available");
        let port = listener.local_addr().expect("the listener is bound").port();
        let request = tokio::spawn(async move {
            // Bounded on both ends. A regression that answers `Delivered`
            // without connecting would otherwise park this task forever and
            // hang the suite, where what it should do is fail — and TCP is
            // free to split a request across reads, so one `read` would fail
            // for a reason that has nothing to do with the daemon.
            let accepted = tokio::time::timeout(ACCEPT_DEADLINE, listener.accept()).await;
            let Ok(Ok((mut socket, _peer))) = accepted else {
                return String::new();
            };
            let received = read_request(&mut socket).await;
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _written = socket.write_all(response.as_bytes()).await;
            let _flushed = socket.flush().await;
            received
        });
        Self {
            base: format!("http://127.0.0.1:{port}"),
            request,
        }
    }

    /// What the daemon sent, once it has sent it.
    async fn received(self) -> String {
        self.request.await.expect("the fake Slack completed")
    }
}

/// One HTTP request, read until its declared body is complete.
///
/// TCP does not promise a request arrives in one read, and this one carries a
/// JSON body — so a single `read` would truncate under a split that has nothing
/// to do with the daemon, and the assertions on the bearer and the channel
/// would fail for the wrong reason.
async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut received = Vec::new();
    let mut chunk = vec![0u8; 4096];
    let deadline = tokio::time::Instant::now() + READ_DEADLINE;
    loop {
        let read = tokio::time::timeout_at(deadline, socket.read(&mut chunk)).await;
        let Ok(Ok(count)) = read else { break };
        if count == 0 {
            break;
        }
        received.extend_from_slice(chunk.get(..count).unwrap_or_default());
        if complete(&received) {
            break;
        }
    }
    String::from_utf8_lossy(&received).into_owned()
}

/// Whether the bytes so far carry the headers and the whole declared body.
fn complete(received: &[u8]) -> bool {
    let text = String::from_utf8_lossy(received);
    let Some(head_len) = text.find("\r\n\r\n") else {
        return false;
    };
    let declared = text
        .get(..head_len)
        .unwrap_or_default()
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")?
                .trim()
                .parse::<usize>()
                .ok()
        })
        .unwrap_or(0);
    received.len() >= head_len + 4 + declared
}

/// A workspace whose fleet asked a question, and the grant that answers it.
struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: Uuid7,
    subject: String,
    user: String,
    fleet: Uuid7,
    event: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            subject: format!("user_live_slack_poster_{}", mint_id()),
            user: mint_id(),
            fleet: Uuid7::parse(&mint_id()).expect("a minted fleet is canonical"),
            event: format!("evt_{}", mint_id()),
            lane,
        }
    }

    fn vault(&self) -> Vault {
        Vault::new(
            self.database.clone(),
            Arc::new(Kek::from_bytes(FIXTURE_KEK)),
            Entropy::new(),
        )
    }

    fn poster(&self, api_base: &str) -> afd_outbound::SlackPoster {
        afd_outbound::SlackPoster::new(
            self.database.clone(),
            Grants::new(self.vault(), self.database.clone(), Entropy::new()),
            reqwest::Client::new(),
            api_base.to_owned(),
        )
    }

    /// The job the queue would have handed the worker.
    fn job(&self) -> OutboundDelivery {
        OutboundDelivery {
            id: EventId::of("1700000000001-0"),
            provider: Provider::Slack.id().to_owned(),
            workspace_id: self.workspace.to_string(),
            fleet_id: self.fleet.to_string(),
            event_id: self.event.clone(),
            answer: ANSWER.to_owned(),
        }
    }

    /// Seeds the tenant, workspace, fleet and the event that asked.
    ///
    /// `request_json` is what the mention ingress wrote when the question
    /// arrived, and it is the only record of where the answer belongs — a
    /// missing one is an answer with nowhere to go, not a retryable blip.
    async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Slack poster live', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'slack-poster', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, 'slack-poster@example.test', 1, 1) \
             ), fleet AS ( \
               INSERT INTO core.fleets \
                 (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                  status, created_at, updated_at) \
               VALUES ($5::uuid, $2::uuid, $1::uuid, 'slack-poster-fleet', '# fixture', \
                       '{}'::jsonb, 'active', 1, 1) \
             ) \
             INSERT INTO core.fleet_events \
               (fleet_id, workspace_id, event_id, actor, event_type, status, \
                request_json, created_at, updated_at) \
             VALUES ($5::uuid, $2::uuid, $6, 'slack:fixture', 'mention', 'succeeded', \
                     $7::jsonb, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(&self.subject)
        .bind(&self.user)
        .bind(self.fleet.as_str())
        .bind(&self.event)
        .bind(format!(
            r#"{{"channel_id":"{CHANNEL}","reply_thread_ts":"{THREAD}"}}"#
        ))
        .execute(&mut *connection)
        .await
        .expect("the tenant, workspace, fleet and asking event seed");
    }

    /// Seals a Slack grant carrying `token`.
    async fn seal_grant(&self, token: &str) {
        let body = format!(r#"{{"integration":"slack","bot_token":"{token}"}}"#);
        let raw = serde_json::value::RawValue::from_string(body)
            .expect("the fixture handle is an object");
        let sealed = self
            .vault()
            .create(
                &self.workspace,
                &SecretName::parse(Provider::Slack.grant_key())
                    .expect("a provider key is storable"),
                &SecretBody::parse(&raw).expect("the fixture handle is a storable body"),
                UnixMillis::from_millis(1),
            )
            .await;
        assert!(sealed.is_ok(), "the fixture grant seals: {sealed:?}");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .expect("the cleanup transaction opens");
        sqlx::query("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid")
            .bind(self.workspace.as_str())
            .execute(&mut *transaction)
            .await
            .expect("the sealed grant cleans up");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *transaction)
            .await
            .expect("the tenant cascades away");
        transaction.commit().await.expect("the cleanup commits");
        drop(connection);
        drop(self.database);
        drop(self.lane);
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_answer_is_threaded_under_the_message_that_asked_it() {
    // The whole read path in one pass, and every field asserted is one the
    // daemon looked up rather than one this test handed it: the bearer is the
    // token the vault opened, and the channel and thread are the row the
    // mention ingress wrote.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;

    let slack = FakeSlack::answering("200 OK", r#"{"ok":true}"#).await;
    let verdict = fixture.poster(&slack.base).deliver(&fixture.job()).await;
    assert_eq!(
        verdict,
        Verdict::Delivered,
        "a Slack that accepted is a delivery"
    );

    let sent = slack.received().await;
    assert!(
        sent.contains(&format!("authorization: Bearer {BOT_TOKEN}"))
            || sent.contains(&format!("Authorization: Bearer {BOT_TOKEN}")),
        "the post must carry the token the vault opened: {sent}"
    );
    assert!(
        sent.contains(CHANNEL) && sent.contains(THREAD),
        "the answer must be threaded under the message that asked it, from the \
         stored request rather than from anywhere else: {sent}"
    );
    assert!(
        sent.contains(ANSWER),
        "the answer itself must be sent: {sent}"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_workspace_holding_no_grant_is_permanent_rather_than_retried() {
    // Uninstalled, disconnected, or a grant that landed malformed. Reconnecting
    // is the only fix, so a retry budget spent here is a queue head blocked on
    // an answer that can never go out.
    let fixture = Fixture::create().await;
    fixture.seed().await;

    let slack = FakeSlack::answering("200 OK", r#"{"ok":true}"#).await;
    let verdict = fixture.poster(&slack.base).deliver(&fixture.job()).await;
    assert_eq!(verdict, Verdict::Permanent);

    drop(slack);
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_answer_to_an_event_no_row_remembers_is_permanent() {
    // Gone, unreadable, or naming nowhere to post — one answer for all three,
    // because a caller does the same thing with each: this is not an event a
    // poster can thread an answer under, and no retry makes it one.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;

    let slack = FakeSlack::answering("200 OK", r#"{"ok":true}"#).await;
    let mut orphaned = fixture.job();
    orphaned.event_id = "evt_no_row_remembers_this".to_owned();
    let verdict = fixture.poster(&slack.base).deliver(&orphaned).await;
    assert_eq!(verdict, Verdict::Permanent);

    drop(slack);
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_two_hundred_that_says_not_ok_is_not_a_delivery() {
    // Slack answers its own failures with 200 and `ok: false`. Reading the
    // status alone would acknowledge an answer that never reached the channel,
    // and the person who asked would be waiting for something already dropped.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;

    let slack = FakeSlack::answering("200 OK", r#"{"ok":false,"error":"channel_not_found"}"#).await;
    let verdict = fixture.poster(&slack.base).deliver(&fixture.job()).await;
    assert_ne!(
        verdict,
        Verdict::Delivered,
        "a 200 carrying `ok: false` is Slack refusing, not accepting"
    );

    let _sent = slack.received().await;
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_vendor_that_is_briefly_unwell_is_retried() {
    // A 5xx is the one class worth spending the budget on, and this is the case
    // that separates it from the two permanent ones above.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;

    let slack = FakeSlack::answering("503 Service Unavailable", r#"{"ok":false}"#).await;
    let verdict = fixture.poster(&slack.base).deliver(&fixture.job()).await;
    assert_eq!(verdict, Verdict::Retryable);

    let _sent = slack.received().await;
    fixture.cleanup().await;
}
