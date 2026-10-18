//! A stored handle mints only what its own workspace connected.
//!
//! The repro these close: workspace A stores a handle through the generic
//! secrets route naming workspace B's installation id, and A's runner asks for
//! a token. The id is a short integer, not a secret, and the App's key signs
//! for any installation it is on — so the routing row the connect flow wrote is
//! the only fact A could not have typed. Its sibling: a refresh handle naming
//! an accounts server of A's choosing, where the platform's client secret
//! would be posted.
//!
//! The vendor is a stand-in [`Exchanger`] that counts what reaches it. A
//! refusal is proven by the code AND by a count of zero, so no case leans on
//! the fixture deployment lacking a platform App to show nothing was signed.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_core::error_code::ErrorCode;
use afd_core::test_util::trace::{Capture, CapturedEvent};
use afd_credential::credential::{Ask, Broker, Exchanger, Minted, Outcome};
use afd_credential::secrets::connector::{Connector, Registry};
use zeroize::Zeroizing;

use super::*;
use crate::requests;
use afd_crypto::entropy::Entropy;

/// The token every counted exchange hands back.
const COUNTED_TOKEN: &str = "ghs_counted";

/// A vendor that mints for whatever reaches it, and remembers how often.
#[derive(Debug, Default)]
struct Counting(AtomicUsize);

impl Exchanger for Counting {
    fn exchange<'a>(
        &'a self,
        _connector: &'a dyn Connector,
        _ask: Ask<'a>,
    ) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(std::future::ready(Outcome::Ok(Minted {
            token: Zeroizing::new(COUNTED_TOKEN.to_owned()),
            expires_at: afd_core::clock::after(afd_core::clock::now(), LEASE_WINDOW),
            rotated_refresh_token: None,
        })))
    }
}

/// A fresh installation id, so cases running at once never contend for one.
///
/// `(provider, external_account_id)` is unique across the table, and every
/// case in this file routes an installation: a shared constant made whichever
/// case inserted second fail at the seed rather than at what it asserts.
fn installation() -> String {
    let mut bytes = [0u8; 4];
    Entropy::new()
        .fill(&mut bytes)
        .expect("the host provides entropy");
    u32::from_be_bytes(bytes).to_string()
}

/// A GitHub handle naming `installation`, as the connect flow writes one.
fn github_handle(installation: &str) -> String {
    format!(r#"{{"integration":"github","installation_id":"{installation}"}}"#)
}

/// A Zoho refresh handle redeemable at `accounts_base`.
fn zoho_handle(accounts_base: &str) -> String {
    format!(r#"{{"integration":"zoho","refresh_token":"rt","accounts_base":"{accounts_base}"}}"#)
}

/// Writes the routing row the GitHub connect flow lands beside its handle.
async fn seed_install(fixtures: &Fixtures, workspace: &str, installation: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO connector.installs
           (id, provider, external_account_id, workspace_id, scopes, created_at, updated_at)
         VALUES ($1::uuid, $2, $3, $4::uuid, '{}', now(), now())",
    )
    .bind(mint_id())
    .bind(CONNECTOR_GITHUB)
    .bind(installation)
    .bind(workspace)
    .execute(&mut *connection)
    .await
    .expect("the routing row must insert");
}

/// A fleet granted `service` whose workspace holds `handle` under `name`.
async fn granted_with(fixtures: &Fixtures, name: &str, service: &str, handle: &str) -> Bound {
    let owner = bound(fixtures, "{}").await;
    seed_handle(fixtures, &owner.workspace, name, handle).await;
    seed_grant(fixtures, &owner.fleet, service, STATUS_APPROVED).await;
    owner
}

/// The operator's record of a refused handle: one warning, naming the
/// workspace and the refusal's code.
fn assert_refusal_logged(capture: &Capture, who: &Bound, code: ErrorCode) {
    let events = capture.events();
    let [warned]: [&CapturedEvent; 1] = events
        .iter()
        .filter(|event| event.field("event") == Some("credential_handle_refused"))
        .collect::<Vec<_>>()
        .try_into()
        .expect("exactly one refused-handle warning");
    assert_eq!(warned.field("workspace_id"), Some(who.workspace.as_str()));
    assert_eq!(warned.field("error_code"), Some(code.as_str()));
}

/// What `who` asking for `integration` is answered, and how many exchanges
/// that answer cost.
async fn mint(
    fixtures: &Fixtures,
    who: &Bound,
    integration: &str,
) -> (std::result::Result<Minted, ErrorCode>, usize) {
    let counting = Arc::new(Counting::default());
    let plane = afd_fleet::lease::Plane {
        broker: Arc::new(Broker::new(
            Arc::new(Registry::default()),
            Arc::clone(&counting) as Arc<dyn Exchanger>,
        )),
        ..fixtures.plane()
    };
    let answer = plane
        .mint(
            &who.runner,
            &requests::mint(&who.lease, integration),
            fixtures.enrolled_at(),
        )
        .await
        .map_err(|refused| refused.code());
    (answer, counting.0.load(Ordering::SeqCst))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_handle_naming_another_workspaces_installation_mints_nothing() {
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let installation = installation();
    let victim = bound(&fixtures, "{}").await;
    seed_install(&fixtures, &victim.workspace, &installation).await;
    let forger = granted_with(
        &fixtures,
        CONNECTOR_GITHUB,
        CONNECTOR_GITHUB,
        &github_handle(&installation),
    )
    .await;

    let capture = Capture::install();
    let (answer, exchanges) = mint(&fixtures, &forger, CONNECTOR_GITHUB).await;

    assert_eq!(answer.err(), Some(error_code::GH_RECONNECT_REQUIRED));
    assert_eq!(exchanges, 0, "a forged handle reached the exchange");
    assert_refusal_logged(&capture, &forger, error_code::GH_RECONNECT_REQUIRED);

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_handle_with_no_routing_row_mints_nothing() {
    // The same refusal as the forged case, on purpose: told apart, the answer
    // would say whether some other workspace holds the installation.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let unrouted = granted_with(
        &fixtures,
        CONNECTOR_GITHUB,
        CONNECTOR_GITHUB,
        &github_handle(&installation()),
    )
    .await;

    let (answer, exchanges) = mint(&fixtures, &unrouted, CONNECTOR_GITHUB).await;

    assert_eq!(answer.err(), Some(error_code::GH_RECONNECT_REQUIRED));
    assert_eq!(exchanges, 0);

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_the_workspaces_own_installation_mints() {
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let installation = installation();
    let owner = granted_with(
        &fixtures,
        CONNECTOR_GITHUB,
        CONNECTOR_GITHUB,
        &github_handle(&installation),
    )
    .await;
    seed_install(&fixtures, &owner.workspace, &installation).await;

    let (answer, exchanges) = mint(&fixtures, &owner, CONNECTOR_GITHUB).await;

    let minted = answer.expect("the owner's own installation mints");
    assert_eq!(minted.token.as_str(), COUNTED_TOKEN);
    assert_eq!(exchanges, 1);

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_github_handle_filed_under_another_name_is_not_connected() {
    // Asked for by a name no connector carries, the grant gate has nothing to
    // check; the handle inside still says GitHub. Even with its workspace
    // holding the installation, it is not minted as a connector nobody granted.
    // The handle IS stored under that name, so this is not a vault miss.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let renamed = "github-copy";
    let installation = installation();
    let owner = granted_with(
        &fixtures,
        renamed,
        CONNECTOR_GITHUB,
        &github_handle(&installation),
    )
    .await;
    seed_install(&fixtures, &owner.workspace, &installation).await;

    let (answer, exchanges) = mint(&fixtures, &owner, renamed).await;

    assert_eq!(
        answer.err(),
        Some(error_code::CRED_INTEGRATION_NOT_CONNECTED)
    );
    assert_eq!(exchanges, 0);

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_refresh_steered_off_its_provider_posts_nothing() {
    // The platform's client secret rides every refresh. A base the SSRF guard
    // admits but no Zoho callback writes must never reach the exchange.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let steered = granted_with(
        &fixtures,
        CONNECTOR_ZOHO,
        CONNECTOR_ZOHO,
        &zoho_handle("https://collector.example"),
    )
    .await;

    let capture = Capture::install();
    let (answer, exchanges) = mint(&fixtures, &steered, CONNECTOR_ZOHO).await;

    assert_eq!(
        answer.err(),
        Some(error_code::CONNECTOR_OAUTH_EXCHANGE_FAILED)
    );
    assert_eq!(exchanges, 0, "a steered refresh reached the exchange");
    assert_refusal_logged(
        &capture,
        &steered,
        error_code::CONNECTOR_OAUTH_EXCHANGE_FAILED,
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_refresh_at_its_own_data_centre_mints() {
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = granted_with(
        &fixtures,
        CONNECTOR_ZOHO,
        CONNECTOR_ZOHO,
        &zoho_handle("https://accounts.zoho.eu"),
    )
    .await;

    let (answer, exchanges) = mint(&fixtures, &owner, CONNECTOR_ZOHO).await;

    assert!(answer.is_ok(), "{:?}", answer.err());
    assert_eq!(exchanges, 1);

    fixtures.cleanup().await;
}
