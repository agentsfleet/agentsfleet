//! Tenant API-key metadata: scoped to its account, free of secrets, read in one page statement.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use af_core::error_code;
use af_core::id::Uuid7;
use af_core::test_util::trace::{Capture, CapturedEvent};
use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{ApiKeySeed, TenantSeed, TestDatabase, mint_id};
use axum::Router;
use http::{Method, StatusCode};
use serde_json::Value;

use crate::harness::{Fleet, error_code as error_code_of, items, text};

/// The account that mints the keys under test.
const OWNER: &str = "user_live_api_key_metadata_owner";
/// A second account, which must learn nothing about the first one's keys.
const STRANGER: &str = "user_live_api_key_metadata_stranger";
/// The production statement that reads one page and the account's total.
const PAGE_STATEMENT_MARK: &str = "tenant_total";
/// The table every API-key statement reads.
const KEY_TABLE: &str = "identity.api_keys";
/// The fields sqlx raises a statement's text under.
const STATEMENT_FIELDS: [&str; 2] = ["summary", "db.statement"];

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn api_key_metadata_is_safe_and_account_scoped() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let owner = Account::seed(&database, OWNER, "Support triage").await;
    let stranger = Account::seed(&database, STRANGER, "Pull request review").await;
    let capture = Capture::install();
    let owners = owner.router(&database);
    let strangers = stranger.router(&database);
    let mut seen = Transcript::default();

    let live = seen.mint(&owners, &owner.token, "triage").await;
    let retired = seen.mint(&owners, &owner.token, "retired").await;
    the_owner_revokes_and_collides(&mut seen, &owners, &owner.token, &retired).await;
    the_stranger_cannot_see_or_touch(&mut seen, &strangers, &stranger, [&live, &retired]).await;
    the_owner_keeps_its_keys(&mut seen, &owners, &live, &retired).await;

    let paging_keys = [
        seen.mint(&owners, &owner.token, "page-one").await,
        seen.mint(&owners, &owner.token, "page-two").await,
    ];
    one_bounded_page_statement(&mut seen, &capture, &owners, &owner.token).await;

    let secrets = secrets_of(
        &database,
        [&live, &retired, &paging_keys[0], &paging_keys[1]],
    )
    .await;
    let (status, _) = seen
        .exchange(
            &owners,
            Method::DELETE,
            &key_path(&retired.id),
            &owner.token,
            "",
        )
        .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "the owner deletes its revoked key"
    );
    seen.assert_carries_none(&live, &secrets);
    assert_logs_carry_none(&capture, &live, &secrets);

    drop(capture);
    owner.cleanup(&database).await;
    stranger.cleanup(&database).await;
    drop(database);
    lane.cleanup().await;
}

/// The owner's own revoke and duplicate-name refusal, whose bodies join the transcript.
async fn the_owner_revokes_and_collides(
    seen: &mut Transcript,
    owners: &Router,
    token: &str,
    retired: &Minted,
) {
    let (status, revoked) = seen
        .exchange(
            owners,
            Method::PATCH,
            &key_path(&retired.id),
            token,
            r#"{"active":false}"#,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "the owner revokes its own key");
    assert_eq!(revoked.get("active").and_then(Value::as_bool), Some(false));

    let (status, duplicate) = seen
        .exchange(
            owners,
            Method::POST,
            "/v1/api-keys",
            token,
            r#"{"key_name":"triage"}"#,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{duplicate}");
    assert_eq!(
        error_code_of(&duplicate),
        Some(error_code::APIKEY_NAME_TAKEN.as_str())
    );
}

/// The stranger lists only its own key, and every verb on the owner's keys
/// answers exactly what an identifier nobody minted answers.
async fn the_stranger_cannot_see_or_touch(
    seen: &mut Transcript,
    strangers: &Router,
    stranger: &Account,
    owned: [&Minted; 2],
) {
    let (status, listed) = seen
        .exchange(strangers, Method::GET, "/v1/api-keys", &stranger.token, "")
        .await;
    assert_eq!(status, StatusCode::OK);
    let ids: Vec<&str> = items(&listed)
        .iter()
        .filter_map(|row| text(row, "id"))
        .collect();
    assert_eq!(
        ids,
        [stranger.key.as_str()],
        "the stranger lists its own key and none of the owner's"
    );
    assert_eq!(listed.get("total").and_then(Value::as_i64), Some(1));

    let [live, retired] = owned;
    for (method, key, body) in [
        (Method::PATCH, &live.id, r#"{"active":false}"#),
        (Method::DELETE, &live.id, ""),
        (Method::DELETE, &retired.id, ""),
    ] {
        let foreign = seen
            .exchange(
                strangers,
                method.clone(),
                &key_path(key),
                &stranger.token,
                body,
            )
            .await;
        let unminted = seen
            .exchange(
                strangers,
                method.clone(),
                &key_path(mint_id().as_str()),
                &stranger.token,
                body,
            )
            .await;
        assert_eq!(
            (unminted.0, error_code_of(&unminted.1)),
            (
                StatusCode::NOT_FOUND,
                Some(error_code::APIKEY_NOT_FOUND.as_str())
            ),
            "{method} on an unminted key: {}",
            unminted.1
        );
        assert_eq!(
            refusal(&foreign),
            refusal(&unminted),
            "{method}: the owner's key is indistinguishable from an unminted one"
        );
    }
}

/// After the stranger's attempts the live key still signs in and is listed
/// active, and the revoked one was not deleted.
async fn the_owner_keeps_its_keys(
    seen: &mut Transcript,
    owners: &Router,
    live: &Minted,
    retired: &Minted,
) {
    let (status, listed) = seen
        .exchange(owners, Method::GET, "/v1/api-keys", &live.key, "")
        .await;
    assert_eq!(status, StatusCode::OK, "the minted key still authenticates");
    let active = |id: &str| {
        items(&listed)
            .iter()
            .find(|row| text(row, "id") == Some(id))
            .and_then(|row| row.get("active"))
            .and_then(Value::as_bool)
    };
    assert_eq!(active(&live.id), Some(true), "{listed}");
    assert_eq!(active(&retired.id), Some(false), "{listed}");
}

/// A capped page carries the account's whole total, and its rows come from
/// one statement whatever the page size.
async fn one_bounded_page_statement(
    seen: &mut Transcript,
    capture: &Capture,
    owners: &Router,
    token: &str,
) {
    // The seeded key and four minted ones.
    let all = 5_usize;
    let before = capture.events().len();
    let (status, first) = seen
        .exchange(owners, Method::GET, "/v1/api-keys?limit=2", token, "")
        .await;
    let small = key_statements(capture.events().into_iter().skip(before));
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(items(&first).len(), 2, "the page stops at the limit");
    assert_eq!(first.get("total").and_then(Value::as_u64), Some(5));
    let cursor = text(&first, "next_cursor").expect("a full page continues");

    let (status, second) = seen
        .exchange(
            owners,
            Method::GET,
            &format!("/v1/api-keys?limit=2&starting_after={cursor}"),
            token,
            "",
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(items(&second).len(), 2);
    assert_eq!(second.get("total").and_then(Value::as_u64), Some(5));
    assert!(
        items(&second)
            .iter()
            .all(|row| !items(&first).contains(row)),
        "the continuation reads past the first page"
    );

    let before = capture.events().len();
    let (status, whole) = seen
        .exchange(owners, Method::GET, "/v1/api-keys?limit=100", token, "")
        .await;
    let large = key_statements(capture.events().into_iter().skip(before));
    assert_eq!(status, StatusCode::OK, "{whole}");
    assert_eq!(items(&whole).len(), all);
    assert!(whole.get("next_cursor").is_none_or(Value::is_null));

    assert_eq!(
        small.pages,
        [Some("2".to_owned())],
        "one page statement returns the two rows"
    );
    assert_eq!(large.pages, [Some(all.to_string())]);
    assert_eq!(
        small.total, large.total,
        "a page of {all} keys costs the statements a page of two does"
    );
}

/// The statements one request ran against the key table.
struct KeyStatements {
    /// How many statements named the table.
    total: usize,
    /// Each page statement's returned row count.
    pages: Vec<Option<String>>,
}

fn key_statements(events: impl Iterator<Item = CapturedEvent>) -> KeyStatements {
    let statement = |event: &CapturedEvent| {
        STATEMENT_FIELDS
            .iter()
            .filter_map(|field| event.field(field))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let touching: Vec<CapturedEvent> = events
        .filter(|event| statement(event).contains(KEY_TABLE))
        .collect();
    KeyStatements {
        total: touching.len(),
        pages: touching
            .iter()
            .filter(|event| statement(event).contains(PAGE_STATEMENT_MARK))
            .map(|event| event.field("rows_returned").map(str::to_owned))
            .collect(),
    }
}

/// Each minted key's plaintext and the digest the table holds for it.
///
/// The digest is read back from the row and checked against [`Digest::of`],
/// so the leak search looks for the value that is really stored.
async fn secrets_of(database: &Db, minted: [&Minted; 4]) -> Vec<String> {
    let mut connection = database.acquire().await.expect("an API connection");
    let mut secrets = Vec::new();
    for key in minted {
        let stored: String =
            sqlx::query_scalar("SELECT key_hash FROM identity.api_keys WHERE id = $1")
                .bind(Uuid7::parse(&key.id).expect("a minted key id is a UUIDv7"))
                .fetch_one(&mut *connection)
                .await
                .expect("the minted key is stored");
        let computed = Digest::of(&Presented::new(&key.key).expect("a minted key is presentable"));
        assert_eq!(
            stored,
            computed.as_str(),
            "the table holds the key's digest"
        );
        secrets.push(key.key.clone());
        secrets.push(stored);
    }
    secrets
}

/// No event or span field the requests raised carries a key or its digest.
fn assert_logs_carry_none(capture: &Capture, live: &Minted, secrets: &[String]) {
    let events = capture.events();
    assert!(
        events.iter().any(|event| {
            event.field("event") == Some("apikey_minted")
                && event.field("key_id") == Some(live.id.as_str())
        }),
        "the capture saw the mint it searches"
    );
    let fields = events
        .iter()
        .flat_map(|event| event.fields.values())
        .chain(capture.spans().iter().flat_map(|span| span.fields.values()))
        .cloned()
        .collect::<Vec<_>>();
    for secret in secrets {
        assert!(
            fields.iter().all(|value| !value.contains(secret.as_str())),
            "a log line carries a key or its digest"
        );
    }
}

/// `/v1/api-keys/{id}`.
fn key_path(id: &str) -> String {
    format!("/v1/api-keys/{id}")
}

/// A refusal's status, registry code and sentence.
fn refusal((status, body): &(StatusCode, Value)) -> (StatusCode, Option<&str>, Option<&str>) {
    (*status, error_code_of(body), text(body, "detail"))
}

/// One key as the mint revealed it.
struct Minted {
    id: String,
    key: String,
}

/// Every response body the test read, as raw text.
///
/// The mint reply is kept with its one-time `key` field removed, the only
/// place a plaintext may appear.
#[derive(Default)]
struct Transcript {
    bodies: Vec<String>,
}

impl Transcript {
    async fn exchange(
        &mut self,
        router: &Router,
        method: Method,
        path: &str,
        token: &str,
        body: &str,
    ) -> (StatusCode, Value) {
        let response = crate::harness::send(router, method, path, Some(token), body).await;
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("a test response body is small and in memory");
        let raw = String::from_utf8(bytes.to_vec()).expect("a response body is UTF-8");
        let answered = if raw.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&raw).expect("a response with a body answers JSON")
        };
        if status != StatusCode::CREATED {
            self.bodies.push(raw);
        }
        (status, answered)
    }

    async fn mint(&mut self, router: &Router, token: &str, name: &str) -> Minted {
        let body = serde_json::json!({"key_name": name, "description": "metadata safety"});
        let (status, mut minted) = self
            .exchange(
                router,
                Method::POST,
                "/v1/api-keys",
                token,
                &body.to_string(),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{minted}");
        let key = minted
            .as_object_mut()
            .and_then(|object| object.remove("key"))
            .and_then(|key| key.as_str().map(str::to_owned))
            .expect("the mint reveals the key once");
        assert!(key.starts_with("agt_t"), "a tenant key carries its class");
        self.bodies.push(minted.to_string());
        Minted {
            id: text(&minted, "id")
                .expect("the mint names its id")
                .to_owned(),
            key,
        }
    }

    fn assert_carries_none(&self, live: &Minted, secrets: &[String]) {
        assert!(
            self.bodies
                .iter()
                .any(|body| body.contains(live.id.as_str())),
            "the transcript holds the bodies that name the key"
        );
        for body in &self.bodies {
            for secret in secrets {
                assert!(
                    !body.contains(secret.as_str()),
                    "a response body carries a key or its digest: {body}"
                );
            }
        }
    }
}

/// One tenant holding one seeded key to call with.
struct Account {
    subject: &'static str,
    tenant: Uuid7,
    key: Uuid7,
    token: String,
}

impl Account {
    async fn seed(database: &Db, subject: &'static str, name: &str) -> Self {
        let bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        let account = Self {
            subject,
            tenant: mint_id(),
            key: mint_id(),
            token: format!("agt_t{bits}"),
        };
        let digest = Digest::of(&Presented::new(&account.token).expect("the token is valid"));
        let mut connection = database.acquire().await.expect("an API connection");
        TenantSeed::new(&account.tenant)
            .name(name)
            .insert(&mut *connection)
            .await
            .expect("the tenant seeds");
        ApiKeySeed::new(&account.key, &account.tenant, digest.as_str(), subject)
            .insert(&mut *connection)
            .await
            .expect("its key seeds");
        account
    }

    /// The production router authenticating this account's subject.
    fn router(&self, database: &Db) -> Router {
        Fleet::live(
            database.clone(),
            self.subject,
            ScopeSet::from_scopes(&Scope::ALL),
        )
        .with_live_ownership()
        .router()
    }

    async fn cleanup(&self, database: &Db) {
        let mut connection = database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped tenant cleans up");
    }
}
