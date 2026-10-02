//! The signup route over a real schema: a verified `user.created` opens one
//! personal account, a replay answers with it, the name is stored without a
//! gap, and the provider is told which tenant it opened.
//!
//! Signed through `identity_signup_route.rs` and built through
//! `identity_signup_events.rs`, so these deliveries are byte for byte what the
//! refusal cases send.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

use afd_auth::scope::ScopeSet;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_identity::MetadataUnwritten;
use http::StatusCode;
use serde_json::{Value, json};

use crate::harness::{Fleet, RecordingWriteback, json_body};
use crate::identity_signup_events::created;
use crate::identity_signup_route::{SECRET, deliver};

/// A daemon over the lane's schema, signed in as `caller` and verifying
/// signups against [`SECRET`], and the pool it reads.
async fn live(caller: &str) -> (Fleet, Db) {
    let database = TestDatabase::shared().open(DbRole::Api, &[]).await;
    let fleet = Fleet::live(database.clone(), caller, ScopeSet::from_scopes(&[]))
        .with_identity_secret(SECRET);
    (fleet, database)
}

/// A subject nobody has signed up yet and an address under it.
///
/// Minted per run so a case does not depend on the schema being reset between
/// runs: `KEEP_TEST_STATE=1` is a supported inner loop, and a fixed subject
/// would make the second run fail as a replay of the first.
fn minted() -> (String, String) {
    let subject = format!("user_{}", mint_id().replace('-', ""));
    let address = format!("{subject}@example.test");
    (subject, address)
}

/// The names the provider sends for Ada.
fn ada_names() -> Value {
    json!({ "first_name": "Ada", "last_name": "Lovelace" })
}

/// The whole point of the endpoint, over a real schema.
///
/// Every case in the two route files is a refusal, and refusals are decided on
/// bytes the handler already holds — none of them reaches the store. That left
/// the one behaviour the route exists for ungraded: a verified `user.created`
/// opening a personal account, and the `Signups` adapter that carries it to
/// `afd_tenant` running only in production.
///
/// # The replay half is the load-bearing half
///
/// An identity provider retries, and the module says a retry must answer as the
/// first delivery did: 200 with `created: false`, naming the SAME workspace.
/// A 409 there would put a delivery the provider cannot change into its retry
/// queue forever, and a second account would give one person two personal
/// workspaces, which nothing downstream can tell apart.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_verified_signup_opens_one_account_and_a_replay_answers_with_it() {
    let (fleet, _database) = live("user_identity_signup_live").await;
    let router = fleet.router();

    let (subject, address) = minted();
    let body = created(&subject, &address, ada_names());

    let opened = json_body(deliver(&router, &body).await).await;
    assert_eq!(
        opened.get("created").and_then(Value::as_bool),
        Some(true),
        "the first delivery of a subject nobody has seen opens the account"
    );
    let workspace = opened
        .get("workspace_id")
        .and_then(Value::as_str)
        .expect("an opened account names its workspace")
        .to_owned();
    assert!(
        !workspace.is_empty(),
        "an account with no workspace is one the person cannot reach"
    );

    let replayed = json_body(deliver(&router, &body).await).await;
    assert_eq!(
        replayed.get("created").and_then(Value::as_bool),
        Some(false),
        "a retry is a success carrying `created: false`, never an error"
    );
    assert_eq!(
        replayed.get("workspace_id").and_then(Value::as_str),
        Some(workspace.as_str()),
        "the replay names the workspace the first delivery opened — a second \
         one would give one person two personal workspaces"
    );
}

/// Every combination of the two name fields the provider may or may not send.
///
/// The provider sends either, both or neither, and the four cases are four
/// different stored values — the one that must not happen is a person stored
/// as `" Lovelace"` or `"Ada "` because an absent half was concatenated anyway.
/// Asserted against the column rather than the response, which does not echo
/// the name: a case that only ran the branch would pass while storing the
/// wrong string.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_name_the_provider_sends_only_half_of_is_stored_without_the_gap() {
    let (fleet, database) = live("user_identity_signup_names").await;
    let router = fleet.router();

    for (given, family, expected) in [
        (Some("Ada"), Some("Lovelace"), Some("Ada Lovelace")),
        (None, Some("Lovelace"), Some("Lovelace")),
        (Some("Ada"), None, Some("Ada")),
        (None, None, None),
    ] {
        let (subject, address) = minted();
        let body = created(
            &subject,
            &address,
            json!({ "first_name": given, "last_name": family }),
        );

        let answer = deliver(&router, &body).await;
        assert_eq!(
            answer.status(),
            StatusCode::OK,
            "given={given:?} family={family:?}"
        );

        let stored: Option<String> =
            sqlx::query_scalar("SELECT display_name FROM core.users WHERE oidc_subject = $1")
                .bind(&subject)
                .fetch_one(&mut *database.acquire().await.expect("a read connection"))
                .await
                .expect("the opened account is readable");

        assert_eq!(
            stored.as_deref(),
            expected,
            "given={given:?} family={family:?} must store {expected:?} — a \
             concatenation around an absent half stores a leading or trailing \
             space nobody typed"
        );
    }
}

/// The writeback the Rust port dropped.
///
/// Signup is TWO writes. The tenant row is the one this daemon owns; the second
/// tells the identity provider which tenant the account resolved to, and until
/// it lands the person's next session token carries no `tenant_id` — so every
/// call they make is refused for want of a tenant context. `identity_events_clerk.zig:290`
/// made that call and the Rust route did not, for the whole of the port: it
/// created tenants and told the provider nothing.
///
/// Nothing failed when it was missing, which is why this test exists rather
/// than a type. The write is best-effort by design — the row is already
/// committed, so a provider outage must not turn signup into a 500 — and an
/// omitted best-effort call produces no error, no 500, and no failing lane.
/// Only an assertion that it HAPPENED can see it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_verified_signup_tells_the_provider_which_tenant_it_opened() {
    let (fleet, _database) = live("user_identity_writeback_live").await;
    let writebacks = fleet.signup_writebacks();
    let router = fleet.router();

    let (subject, address) = minted();
    let body = created(&subject, &address, ada_names());

    let opened = json_body(deliver(&router, &body).await).await;
    assert_eq!(
        opened.get("created").and_then(Value::as_bool),
        Some(true),
        "the case needs a fresh account, not a replay"
    );

    let written = writebacks.written();
    assert_eq!(
        written.len(),
        1,
        "one account opened is one writeback — a second would mean the handler \
         wrote on the replay path too"
    );
    let Some(wrote) = written.first() else {
        // Unreachable past the length assertion; spelled as a fallible read
        // because this crate's suites index nothing.
        return;
    };
    assert_eq!(
        wrote.subject, subject,
        "the write addresses the subject the event named — an account repaired \
         under a different one is a different person's"
    );
    assert!(
        !wrote.tenant_id.is_empty(),
        "a writeback carrying no tenant is the bug it exists to prevent: the \
         provider merges an empty claim and the next token still has none"
    );
    assert_eq!(
        wrote.scopes,
        afd_auth::scope::signup_owner_claim(),
        "the owner grant is what makes the account's first workspace usable; \
         `signup_owner_claim` had NO production caller before this write"
    );
}

/// The provider refusing the writeback does not refuse the delivery.
///
/// The tenant row is already committed when the write runs, so a refusal there
/// answered to the provider would refuse an account that exists and invite a
/// retry that can only duplicate work. The handler swallows it and LOGS it,
/// and this case proves the swallow from both sides: the seam refuses every
/// write, the delivery still answers 200 with the account it opened, and the
/// seam shows the write was ATTEMPTED — a handler that skipped the call would
/// pass a status-only assertion while leaving the operator nothing to repair
/// from.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_provider_that_will_not_take_the_writeback_does_not_refuse_the_delivery() {
    let refusing = RecordingWriteback::refusing(MetadataUnwritten::Unauthorized);
    let (fleet, _database) = live("user_identity_writeback_refused").await;
    let router = fleet.with_signup_writeback(refusing.clone()).router();

    let (subject, address) = minted();
    let body = created(&subject, &address, json!({}));

    let answer = deliver(&router, &body).await;
    assert_eq!(
        answer.status(),
        StatusCode::OK,
        "the row is committed before the write runs; a refused write must not \
         turn an opened account into a delivery the provider retries"
    );
    let opened = json_body(answer).await;
    assert_eq!(
        opened.get("created").and_then(Value::as_bool),
        Some(true),
        "the case needs a fresh account, not a replay"
    );
    assert_eq!(
        refusing.written().len(),
        1,
        "the write was attempted and refused — a handler that never asked \
         would be a skipped write wearing a swallowed one's clothes"
    );
}

/// A subject the account model tolerated and the write cannot address.
///
/// `bootstrap` opens an account under whatever subject the provider sent: the
/// column is `TEXT NOT NULL`, and a run of spaces satisfies it. The writeback
/// cannot follow — a blank subject resolves to nobody at the provider — so the
/// handler declines the write rather than asking the provider to merge a claim
/// into no one. The delivery is still answered, because the row is committed;
/// what the operator has is the log line. Proven by the seam recording
/// NOTHING rather than by a status, since a status alone cannot tell a
/// declined write from one that was never reached.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_subject_that_is_only_whitespace_opens_the_account_but_is_not_written_back() {
    let (fleet, _database) = live("user_identity_writeback_blank").await;
    let writebacks = fleet.signup_writebacks();
    let router = fleet.router();

    // The subject is fixed — it is the whole point — and the address is minted,
    // so a `KEEP_TEST_STATE=1` rerun replays the same account rather than
    // colliding on a second one. The replay path reaches the same write.
    let address = format!("blank-{}@example.test", mint_id().replace('-', ""));
    let body = created("   ", &address, json!({}));

    let answer = deliver(&router, &body).await;
    assert_eq!(
        answer.status(),
        StatusCode::OK,
        "the account model took the subject, so the delivery is answered; the \
         gap is the provider's to see in the log, not a refusal to retry"
    );
    assert!(
        writebacks.written().is_empty(),
        "a blank subject addresses nobody — the write must be declined, not \
         sent for the provider to merge a claim into no one"
    );
}
