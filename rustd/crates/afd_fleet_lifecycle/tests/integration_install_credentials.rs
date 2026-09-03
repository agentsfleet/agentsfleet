//! Dimension 4.4 — an install into a workspace short a credential writes no row.
//!
//! `#[ignore]`d for the reason the rollback suite gives: the claim is about
//! `core.fleets` and `vault.secrets` together, and only a real database can
//! decide it.
//!
//! # Why this cannot be a unit test
//!
//! The refusal itself is a set difference, and a set difference is not what
//! goes wrong. What goes wrong is ORDER: a check that runs after the insert
//! leaves a fleet nobody can run, and the only witness to that is the row count
//! after a refused install. A fake vault would prove the difference and not the
//! order.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::error_code;
use afd_fleet_lifecycle::{ConfigSource, Install, LibrarySource, Patch};

use crate::support::Lane;

/// A bundle whose trigger declares one credential.
const LIBRARY_ID_DEMANDING: &str = "credential-demanding";

/// The credential that bundle names.
const DECLARED_CREDENTIAL: &str = "github";

/// A credential the workspace holds and the bundle never asked for.
const UNRELATED_CREDENTIAL: &str = "postmark";

/// A credential only the EDITED configuration names.
const SECOND_CREDENTIAL: &str = "stripe";

/// A replacement `TRIGGER.md` declaring the stored credential and one more.
const TRIGGER_MD_SECOND_SECRET: &str = "---\nname: credential-demanding\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  credentials:\n    - github\n    - stripe\n  budget:\n    daily_dollars: 1.0\n---\n";

/// That bundle's `SKILL.md`.
const SKILL_MD: &str = "---\nname: credential-demanding\nversion: 1.0.0\ndescription: needs a credential\n---\n\n# Body\n";

/// That bundle's `TRIGGER.md`, naming the credential in its frontmatter.
const TRIGGER_MD: &str = "---\nname: credential-demanding\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  credentials:\n    - github\n  budget:\n    daily_dollars: 1.0\n---\n";

/// The install request every test here makes.
fn request() -> Install<'static> {
    Install {
        source: LibrarySource::Platform(LIBRARY_ID_DEMANDING),
        name: None,
    }
}

/// A workspace holding none of the declared credentials installs nothing.
#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_workspace_short_the_declared_credential_gets_no_fleet() {
    let lane = Lane::create().await;
    lane.seed_library_entry(LIBRARY_ID_DEMANDING, SKILL_MD, Some(TRIGGER_MD))
        .await;
    let before = lane.fleet_count(&lane.workspace).await;

    let refused = lane
        .fleets
        .install(&lane.workspace, &request(), Lane::now())
        .await
        .expect_err("a bundle naming a credential nobody stored must not install");

    assert_eq!(refused.code(), error_code::FLEET_BUNDLE_SECRETS_MISSING);
    assert_eq!(
        refused.missing_secrets(),
        Some([DECLARED_CREDENTIAL.to_owned()].as_slice()),
        "the refusal names what to add, so an operator does not diff the \
         bundle against their own vault by hand"
    );
    assert!(
        !refused.is_datastore_unavailable(),
        "the workspace is not ready; this instance is fine"
    );
    assert_eq!(
        lane.fleet_count(&lane.workspace).await,
        before,
        "the refusal must land BEFORE the row: a fleet installed short a \
         credential is one that cannot run, discovered at the first lease"
    );

    lane.cleanup().await;
}

/// A credential the bundle did not ask for does not satisfy one it did.
#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn an_unrelated_credential_does_not_satisfy_the_declared_one() {
    let lane = Lane::create().await;
    lane.seed_library_entry(LIBRARY_ID_DEMANDING, SKILL_MD, Some(TRIGGER_MD))
        .await;
    lane.seed_secret(UNRELATED_CREDENTIAL).await;

    let refused = lane
        .fleets
        .install(&lane.workspace, &request(), Lane::now())
        .await
        .expect_err("holding some other credential is not holding this one");

    assert_eq!(
        refused.missing_secrets(),
        Some([DECLARED_CREDENTIAL.to_owned()].as_slice()),
        "a count-based check would pass here"
    );

    lane.cleanup().await;
}

/// Once the credential is stored, the same install goes through.
///
/// The pre-flight has to be a gate rather than a wall: a test that only ever
/// saw the refusal would pass against a check that refused everything.
#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn the_same_install_succeeds_once_the_credential_is_stored() {
    let lane = Lane::create().await;
    lane.seed_library_entry(LIBRARY_ID_DEMANDING, SKILL_MD, Some(TRIGGER_MD))
        .await;
    lane.seed_secret(DECLARED_CREDENTIAL).await;

    let installed = lane
        .fleets
        .install(&lane.workspace, &request(), Lane::now())
        .await
        .expect("a workspace holding the declared credential installs");

    assert_eq!(
        lane.fleet_column(&installed.id, "status").await.as_deref(),
        Some("active")
    );

    lane.cleanup().await;
}

/// The edit door asks the same question the install door does.
///
/// A replacement `TRIGGER.md` declares credentials, so an edit that skipped the
/// check would land exactly the fleet the install refuses — by the other door,
/// and on a fleet the operator can already see.
#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn an_edit_naming_a_credential_the_workspace_lacks_is_refused_too() {
    let lane = Lane::create().await;
    lane.seed_library_entry(LIBRARY_ID_DEMANDING, SKILL_MD, Some(TRIGGER_MD))
        .await;
    lane.seed_secret(DECLARED_CREDENTIAL).await;
    let installed = lane
        .fleets
        .install(&lane.workspace, &request(), Lane::now())
        .await
        .expect("the install goes through while the credential is stored");

    let refused = lane
        .fleets
        .patch(
            &lane.workspace,
            &installed.id,
            &Patch {
                config: Some(ConfigSource::Trigger(TRIGGER_MD_SECOND_SECRET.to_owned())),
                ..Patch::default()
            },
            Lane::now(),
        )
        .await
        .expect_err("an edit naming an unstored credential must not land");

    assert_eq!(refused.code(), error_code::FLEET_BUNDLE_SECRETS_MISSING);
    assert_eq!(
        refused.missing_secrets(),
        Some([SECOND_CREDENTIAL.to_owned()].as_slice()),
        "the stored one is not missing; only the newly declared one is"
    );

    lane.cleanup().await;
}
