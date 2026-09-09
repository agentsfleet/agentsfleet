//! An install asks for the grants the bundle says the fleet will need.
//!
//! # Why this cannot be a unit test
//!
//! The claim spans three tables and a sealed envelope. The bundle names
//! `github`; the workspace's stored handle for it says whether that is a
//! connector the daemon must MINT against, and only opening the envelope
//! answers that; and the request writes `core.integration_grants` and
//! `core.fleet_approval_gates` together. A fake vault would prove the
//! classifier, which `afd_credential` already proves; what is proven here is
//! that the install RUNS it, on real bytes, and that the rows land.
//!
//! # And why a static credential is asserted beside it
//!
//! The request must not widen past `mintable()`. A bundle declaring `elastic`
//! or `grafana` — an api key used as it stands, never brokered — would
//! otherwise raise a card nobody can act on, for a decision no mint consults.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_approval::{KIND_INTEGRATION_GRANT, REASON_DECLARED_AT_INSTALL};
use afd_wire::grant::status;
use sqlx::Row as _;

use crate::support::{GITHUB_HANDLE, Lane, STATIC_HANDLE};

/// A bundle whose trigger declares one credential the workspace can mint.
const LIBRARY_ID_MINTING: &str = "grant-wanting";

/// The credential that bundle names.
const DECLARED_CREDENTIAL: &str = "github";

/// The connector behind it — what the grant row and the card both name.
const SERVICE: &str = "github";

/// A bundle whose trigger declares a credential that ships as it stands.
const LIBRARY_ID_STATIC: &str = "static-wanting";

/// The credential THAT bundle names.
const STATIC_CREDENTIAL: &str = "elastic";

/// The minting bundle's `SKILL.md`.
///
/// One per bundle, and the name in each has to match its own `TRIGGER.md`:
/// the install cross-checks the two documents and refuses a pair that names
/// two different fleets. A shared `SKILL.md` here failed exactly that way.
const SKILL_MD_MINTING: &str =
    "---\nname: grant-wanting\nversion: 1.0.0\ndescription: needs a grant\n---\n\n# Body\n";

/// The static bundle's.
const SKILL_MD_STATIC: &str =
    "---\nname: static-wanting\nversion: 1.0.0\ndescription: needs no grant\n---\n\n# Body\n";

/// `TRIGGER.md` declaring the mintable credential.
const TRIGGER_MD_MINTING: &str = "---\nname: grant-wanting\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  credentials:\n    - github\n  budget:\n    daily_dollars: 1.0\n---\n";

/// `TRIGGER.md` declaring the static one.
const TRIGGER_MD_STATIC: &str = "---\nname: static-wanting\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  credentials:\n    - elastic\n  budget:\n    daily_dollars: 1.0\n---\n";

/// The install request, against `library`.
fn request(library: &str) -> afd_fleet_lifecycle::Install<'_> {
    afd_fleet_lifecycle::Install {
        source: afd_fleet_lifecycle::LibrarySource::Platform(library),
        name: None,
    }
}

/// Every grant row a fleet holds, as `(service, status, reason)`.
async fn grant_rows(lane: &Lane, fleet: &str) -> Vec<(String, String, String)> {
    sqlx::query(
        "SELECT service, status, requested_reason
           FROM core.integration_grants WHERE fleet_id = $1::uuid",
    )
    .bind(fleet)
    .fetch_all(&mut *lane.connection().await)
    .await
    .expect("the grant rows are readable")
    .iter()
    .map(|row| {
        (
            row.try_get(0).expect("service"),
            row.try_get(1).expect("status"),
            row.try_get(2).expect("reason"),
        )
    })
    .collect()
}

/// The services this fleet's grant cards name.
async fn carded_services(lane: &Lane, fleet: &str) -> Vec<Option<String>> {
    sqlx::query(
        "SELECT evidence->>'service' FROM core.fleet_approval_gates
          WHERE fleet_id = $1::uuid AND gate_kind = $2",
    )
    .bind(fleet)
    .bind(KIND_INTEGRATION_GRANT)
    .fetch_all(&mut *lane.connection().await)
    .await
    .expect("the gate rows are readable")
    .iter()
    .map(|row| row.try_get(0).expect("evidence service"))
    .collect()
}

/// Installing a bundle that declares a mintable credential raises its card.
#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn install_requests_a_grant_per_declared_mintable_credential() {
    let lane = Lane::create().await;
    lane.seed_library_entry(
        LIBRARY_ID_MINTING,
        SKILL_MD_MINTING,
        Some(TRIGGER_MD_MINTING),
    )
    .await;
    lane.seal_secret(DECLARED_CREDENTIAL, GITHUB_HANDLE).await;

    let installed = lane
        .fleets
        .install(&lane.workspace, &request(LIBRARY_ID_MINTING), Lane::now())
        .await
        .expect("a workspace holding the credential installs");

    let fleet = installed.id.as_str();
    assert_eq!(
        grant_rows(&lane, fleet).await,
        vec![(
            SERVICE.to_owned(),
            status::PENDING.to_owned(),
            REASON_DECLARED_AT_INSTALL.to_owned()
        )],
        "the fleet leaves install with the decision already waiting, rather \
         than discovering it needs one on a poll nobody is watching"
    );
    assert_eq!(
        carded_services(&lane, fleet).await,
        vec![Some(SERVICE.to_owned())]
    );

    lane.cleanup().await;
}

/// A credential that ships as it stands asks for nothing.
///
/// The regression guard. `mintable()` short-circuits on a handle with no
/// `integration` field, and a request that widened past it would raise cards
/// for `elastic`, `grafana` and `fly` — decisions no credential mint ever
/// consults, on a page an operator has to clear by hand.
#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_non_mintable_declaration_requests_no_grant() {
    let lane = Lane::create().await;
    lane.seed_library_entry(LIBRARY_ID_STATIC, SKILL_MD_STATIC, Some(TRIGGER_MD_STATIC))
        .await;
    lane.seal_secret(STATIC_CREDENTIAL, STATIC_HANDLE).await;

    let installed = lane
        .fleets
        .install(&lane.workspace, &request(LIBRARY_ID_STATIC), Lane::now())
        .await
        .expect("a static credential installs like any other");

    let fleet = installed.id.as_str();
    assert_eq!(grant_rows(&lane, fleet).await, Vec::new());
    assert_eq!(carded_services(&lane, fleet).await, Vec::new());

    lane.cleanup().await;
}

/// Installing the same bundle twice leaves one grant per fleet, and two fleets.
///
/// The idempotence here is NOT the unique constraint — two installs are two
/// fleets and two legitimately different rows. What is proven is that each
/// fleet's own request is written once and names only its own fleet, which is
/// the property a shared-row implementation would break silently.
#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_second_install_asks_for_its_own_fleets_grant() {
    let lane = Lane::create().await;
    lane.seed_library_entry(
        LIBRARY_ID_MINTING,
        SKILL_MD_MINTING,
        Some(TRIGGER_MD_MINTING),
    )
    .await;
    lane.seal_secret(DECLARED_CREDENTIAL, GITHUB_HANDLE).await;

    let first = lane
        .fleets
        .install(&lane.workspace, &request(LIBRARY_ID_MINTING), Lane::now())
        .await
        .expect("the first install succeeds");
    let second = lane
        .fleets
        .install(&lane.workspace, &request(LIBRARY_ID_MINTING), Lane::now())
        .await
        .expect("the second install succeeds under a drawn name");

    assert_ne!(first.id.as_str(), second.id.as_str());
    assert_eq!(grant_rows(&lane, first.id.as_str()).await.len(), 1);
    assert_eq!(grant_rows(&lane, second.id.as_str()).await.len(), 1);

    lane.cleanup().await;
}
