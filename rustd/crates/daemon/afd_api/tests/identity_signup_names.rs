//! How a signup's names and subject are stored, over a real schema. Companion
//! to `identity_signup_live.rs`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

use afd_db::test_util::mint_id;
use http::StatusCode;
use serde_json::json;

use crate::identity_signup_events::created;
use crate::identity_signup_live::{live, minted};
use crate::identity_signup_route::deliver;

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
            sqlx::query_scalar("SELECT display_name FROM identity.users WHERE oidc_subject = $1")
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
    let address = format!("blank-{}@example.test", mint_id().as_str().replace('-', ""));
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
