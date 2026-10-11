//! A rotated refresh token is written back before the caller has its token,
//! and a write-back that fails never turns the mint into a refusal.
//!
//! The vendor is a stand-in [`Exchanger`] — the broker's own seam — that
//! answers every exchange with a fresh access token and a ROTATED refresh
//! token, the way a rotating provider does. What it can also do is reach the
//! stored row while the exchange is in flight, which is the only way the
//! write-back's failure arm is reachable without a datastore that misbehaves.

use std::pin::Pin;
use std::sync::Arc;

use afd_core::test_util::trace::{Capture, CapturedEvent};
use afd_credential::credential::{Ask, Broker, Exchanger, Minted, Outcome};
use afd_credential::secrets::connector::{Connector, Registry};
use afd_credential::vault::KeyRef;
use afd_db::Db;
use serde_json::Value;
use tracing::Level;
use zeroize::Zeroizing;

use super::*;
use crate::requests;

/// The refresh token the stored handle holds when the exchange posts it.
const POSTED: &str = "rt-posted";

/// The refresh token the stand-in vendor rotates it to.
const REPLACEMENT: &str = "rt-replacement";

/// The event the mint's write-back breadcrumb is logged under, as
/// `afd_fleet::lease::mint` spells it (`EVENT_REFRESH_ROTATED`).
const EVENT_REFRESH_ROTATED: &str = "refresh_rotated";

/// The accounts server a European Zoho callback writes onto the handle.
const EU_ACCOUNTS: &str = "https://accounts.zoho.eu";

/// The access token every stand-in exchange hands back.
const ACCESS: &str = "at-rotated";

/// What the stand-in vendor does to the stored row while its exchange runs.
#[derive(Debug, Clone, Copy)]
enum During {
    /// Leaves it alone, as every honest exchange does.
    Nothing,
    /// Breaks its envelope's tag, as a concurrent writer under another key
    /// would leave it — so the write-back cannot open what it must rewrite.
    Tamper,
}

/// A rotating vendor that answers every exchange the same way.
#[derive(Debug)]
struct Rotating {
    database: Db,
    workspace: String,
    during: During,
}

impl Exchanger for Rotating {
    fn exchange<'a>(
        &'a self,
        _connector: &'a dyn Connector,
        _ask: Ask<'a>,
    ) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
        Box::pin(async move {
            if matches!(self.during, During::Tamper) {
                let mut connection = self.database.acquire().await.expect("a pooled connection");
                sqlx::query(
                    "UPDATE vault.secrets SET tag = decode(repeat('00', 16), 'hex')
                     WHERE workspace_id = $1::uuid AND key_name = $2",
                )
                .bind(&self.workspace)
                .bind(CONNECTOR_ZOHO)
                .execute(&mut *connection)
                .await
                .expect("the stored row must update");
            }
            Outcome::Ok(Minted {
                token: Zeroizing::new(ACCESS.to_owned()),
                expires_at_ms: NOW_MS + LEASE_WINDOW_MS,
                rotated_refresh_token: Some(Zeroizing::new(REPLACEMENT.to_owned())),
            })
        })
    }
}

/// Mints the Zoho credential for a granted fleet whose workspace holds
/// `handle`, through a vendor that does `during` mid-exchange.
async fn mint_rotating(fixtures: &Fixtures, handle: &str, during: During) -> (Bound, Minted) {
    let owner = bound(fixtures, "{}").await;
    seed_handle(fixtures, &owner.workspace, CONNECTOR_ZOHO, handle).await;
    seed_grant(fixtures, &owner.fleet, CONNECTOR_ZOHO, STATUS_APPROVED).await;
    let vendor = Rotating {
        database: fixtures.database.clone(),
        workspace: owner.workspace.clone(),
        during,
    };
    let plane = afd_fleet::lease::Plane {
        broker: Arc::new(Broker::new(Arc::new(Registry::default()), Arc::new(vendor))),
        ..fixtures.plane()
    };

    let minted = plane
        .mint(
            &owner.runner,
            &requests::mint(&owner.lease, CONNECTOR_ZOHO),
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect("a rotating exchange mints whatever the write-back does");
    (owner, minted)
}

/// The stored handle's fields, opened the way the next mint will open them.
async fn stored(fixtures: &Fixtures, owner: &Bound) -> Value {
    let workspace = Uuid7::parse(&owner.workspace).expect("a seeded workspace id");
    let held = fixtures
        .plane()
        .vault
        .open(KeyRef {
            workspace_id: &workspace,
            name: CONNECTOR_ZOHO,
        })
        .await
        .expect("the vault answers")
        .expect("the handle is still stored");
    serde_json::from_slice(held.expose()).expect("the stored handle is JSON")
}

/// The one write-back breadcrumb `capture` saw, at `level`, naming `owner`.
///
/// Asserted by value as well as presence: the line is an operator's only view
/// of a rotation, so it must say whose and which, and never the token.
fn breadcrumb(capture: &Capture, level: Level, owner: &Bound) -> CapturedEvent {
    let events = capture.events();
    let [found]: [&CapturedEvent; 1] = events
        .iter()
        .filter(|event| event.field("event") == Some(EVENT_REFRESH_ROTATED))
        .collect::<Vec<_>>()
        .try_into()
        .expect("exactly one write-back breadcrumb");
    assert_eq!(found.level, level, "{found:?}");
    assert_eq!(found.field("workspace_id"), Some(owner.workspace.as_str()));
    assert_eq!(found.field("integration"), Some(CONNECTOR_ZOHO));
    assert!(
        found
            .fields
            .values()
            .all(|value| !value.contains(REPLACEMENT) && !value.contains(ACCESS)),
        "a token rode the breadcrumb: {found:?}"
    );
    found.clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_rotated_refresh_token_is_written_back_before_the_mint_returns() {
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let handle = format!(
        r#"{{"integration":"zoho","refresh_token":"{POSTED}","accounts_base":"{EU_ACCOUNTS}"}}"#
    );

    let capture = Capture::install();
    let (owner, minted) = mint_rotating(&fixtures, &handle, During::Nothing).await;

    assert_eq!(minted.token.as_str(), ACCESS);
    breadcrumb(&capture, Level::DEBUG, &owner);
    let after = stored(&fixtures, &owner).await;
    assert_eq!(
        after.get("refresh_token").and_then(Value::as_str),
        Some(REPLACEMENT),
        "{after}"
    );
    // A rotation, not a rewrite: what the connect callback stored survives.
    assert_eq!(
        after.get("accounts_base").and_then(Value::as_str),
        Some(EU_ACCOUNTS),
        "{after}"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_handle_that_posted_no_refresh_token_is_left_as_stored() {
    // Nothing to compare the stored row against, so nothing is written: a
    // replacement with no posted value could overwrite a reconnect's token.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let handle = format!(r#"{{"integration":"zoho","accounts_base":"{EU_ACCOUNTS}"}}"#);

    let (owner, minted) = mint_rotating(&fixtures, &handle, During::Nothing).await;

    assert_eq!(minted.token.as_str(), ACCESS);
    let after = stored(&fixtures, &owner).await;
    assert!(after.get("refresh_token").is_none(), "{after}");

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_failed_write_back_still_hands_over_the_minted_token() {
    // The vendor already consumed the posted token and issued a working access
    // token; refusing now would cost the run a credential it holds.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let handle = format!(r#"{{"integration":"zoho","refresh_token":"{POSTED}"}}"#);

    let capture = Capture::install();
    let (owner, minted) = mint_rotating(&fixtures, &handle, During::Tamper).await;

    assert_eq!(minted.token.as_str(), ACCESS);
    let warned = breadcrumb(&capture, Level::WARN, &owner);
    assert!(
        warned
            .field("error_code")
            .is_some_and(|code| code.starts_with("UZ-")),
        "{warned:?}"
    );
    assert!(
        warned
            .field("reason")
            .is_some_and(|reason| !reason.is_empty() && !reason.contains("[UZ-")),
        "the reason is the fault's sentence, its code kept out of it: {warned:?}"
    );
    assert_eq!(
        minted.rotated_refresh_token.as_deref().map(String::as_str),
        Some(REPLACEMENT),
        "the caller still learns the replacement it could not persist"
    );

    fixtures.cleanup().await;
}
