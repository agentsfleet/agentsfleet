//! An install writes the grants the bundle says the fleet will need, answered.
//!
//! # Why this cannot be a unit test
//!
//! The claim spans three tables and a sealed envelope. The bundle names
//! `github`; the workspace's stored handle for it says whether that is a
//! connector the daemon must MINT against, and only opening the envelope
//! answers that; and the install writes `core.integration_grants`. A fake vault
//! would prove the classifier, which `afd_credential` already proves; what is
//! proven here is that the install RUNS it, on real bytes, and that the row
//! lands approved with no card beside it.
//!
//! # And why a static credential is asserted beside it
//!
//! The write must not widen past `mintable()`. A bundle declaring `elastic`
//! or `grafana` — an api key used as it stands, never brokered — would
//! otherwise hold a standing authorisation no mint ever consults.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_approval::{
    IntegrationGrants, KIND_INTEGRATION_GRANT, Origin, REASON_DECLARED_AT_INSTALL, Wanted,
};
use afd_crypto::entropy::Entropy;
use afd_wire::grant::status;
use sqlx::Row as _;

use crate::support::{GITHUB_HANDLE, Lane, STATIC_HANDLE};

/// A bundle whose trigger declares one credential the workspace can mint.
const LIBRARY_ID_MINTING: &str = "grant-wanting";

/// The credential that bundle names.
const DECLARED_CREDENTIAL: &str = "github";

/// The connector behind it — what the install's grant row names.
const SERVICE: &str = "github";

/// A connector the bundle does NOT declare.
///
/// The park path raises a card only for a grant that is still `pending`, and
/// the install answers everything the bundle declared — so a card has to be
/// asked for on behalf of something it did not. That is the real shape too: a
/// credential added by a later PATCH reaches the lease path with no grant, and
/// the backstop asks there.
const UNDECLARED_SERVICE: &str = "slack";

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
        mention: None,
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

/// Installing a bundle that declares a mintable credential answers it.
///
/// This asserted a PENDING row and a card until M202. Installing the fleet is
/// the answer — the operator chose it, the bundle names the integration, and
/// the fleet's binding names the repositories and the access level — so the row
/// lands approved and no card is raised. Asking again per event is what put one
/// approval card on every model turn and posted no reviews.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn install_answers_the_grant_for_each_declared_mintable_credential() {
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
            status::APPROVED.to_owned(),
            REASON_DECLARED_AT_INSTALL.to_owned()
        )],
        "the fleet leaves install already authorised, rather than discovering \
         it needs a decision on a poll nobody is watching"
    );
    assert_eq!(
        carded_services(&lane, fleet).await,
        Vec::<Option<String>>::new(),
        "an install raises no card: nobody is asked a question they answered \
         by installing the fleet"
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
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
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
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
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

/// A fleet carrying a grant card can still be deleted.
///
/// `core.fleet_approval_gates` is append-only by trigger (schema/810) and
/// refuses every DELETE unless the purge says it means it, and the cascade from
/// the fleet row fires the same trigger. A fleet holding no gate rows purges
/// clean and never reaches that opt-in; this test is the one that reaches it.
///
/// The card came from the INSTALL until M202, which no longer raises one — and
/// asking the park path for a service the install already granted raises none
/// either, because a standing yes is not re-asked. So the card here is asked
/// for on behalf of a credential the bundle never declared, which is the shape
/// a later PATCH produces. Deleting this test instead would re-open the
/// regression the purge opt-in exists to prevent, across every fleet that holds
/// a gate row for any reason.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_fleet_carrying_a_grant_card_still_purges() {
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
        .expect("the install must land");
    let fleet = installed.id.as_str().to_owned();
    IntegrationGrants::new(lane.pool.clone(), Entropy::new())
        .request(
            &lane.workspace,
            &installed.id,
            Wanted {
                service: UNDECLARED_SERVICE,
                credential: UNDECLARED_SERVICE,
                origin: Origin::Park,
            },
            Lane::now(),
        )
        .await
        .expect("the park backstop raises the card this test purges past");
    assert_eq!(
        carded_services(&lane, &fleet).await,
        vec![Some(UNDECLARED_SERVICE.to_owned())],
        "the precondition: a card exists for the purge to step over"
    );

    lane.fleets
        .patch(
            &lane.workspace,
            &installed.id,
            &afd_fleet_lifecycle::Patch {
                status: Some(afd_fleet_lifecycle::Requested::Killed),
                ..afd_fleet_lifecycle::Patch::default()
            },
            Lane::now(),
        )
        .await
        .expect("active to killed is legal");

    lane.fleets
        .purge(&lane.workspace, &installed.id)
        .await
        .expect("a fleet holding a grant card must still purge");

    assert_eq!(lane.fleet_count(&lane.workspace).await, 0);
    assert_eq!(grant_rows(&lane, &fleet).await, Vec::new());
    assert_eq!(carded_services(&lane, &fleet).await, Vec::new());
    lane.cleanup().await;
}

#[path = "integration_install_grants/recovery.rs"]
mod recovery;
