//! §3 Dimensions 3.2 and 3.3 — what a PATCH changes, and whose fleet it may change.
//!
//! `#[ignore]`d; `make test-integration-rustd` runs them.
//!
//! # 3.2, stated as what the lease path actually reads
//!
//! "Visible on next lease resolve, not before" is a claim about the stored
//! `config_json` column, because that is what a lease reads — `agentsfleetd`
//! resolves a fleet's status and configuration fresh from Postgres on every
//! lease, which is why the PATCH sends no signal to anybody. So the proof is:
//! the row holds the OLD configuration until the PATCH commits, and the NEW one
//! immediately after, with nothing in between and nothing notified.
//!
//! # 3.3's other half
//!
//! The unit suite pins the 403 — a foreign workspace refused by the ownership
//! layer before a statement runs. This is the 404: a workspace the caller DOES
//! own, naming a fleet that lives in another one. That refusal is made by the
//! predicate rather than by a layer, so only a seeded row can prove it.
//!
//! # And the compare-and-set
//!
//! The PATCH takes no row lock. Its safety is a predicate, and a predicate is
//! worth exactly what a concurrent race says it is — so two conditional writes
//! against one fleet run here for real.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::error_code;
use afd_fleet_lifecycle::{ConfigSource, Install, LibrarySource, Patch, Requested};

use crate::support::{LIBRARY_ID, Lane, TRIGGER_MD_EDITED};

/// The install every test here starts from.
async fn installed(lane: &Lane) -> afd_fleet_lifecycle::Installed {
    lane.fleets
        .install(
            &lane.workspace,
            &Install {
                source: LibrarySource::Platform(LIBRARY_ID),
                name: None,
            },
            Lane::now(),
        )
        .await
        .expect("a seeded library entry installs")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Redis"]
async fn a_config_patch_is_in_the_row_the_next_lease_reads_and_not_before() {
    // Dimension 3.2. The daemon resolves configuration from Postgres per lease,
    // so "takes effect on next lease" IS "the column changed" — there is no
    // cache to invalidate and no signal to send, which is the property this
    // pins. A regression that added a cache would leave this green and the
    // system wrong, so the assertion is deliberately on the column.
    let lane = Lane::create().await;
    let fleet = installed(&lane).await;

    let before = lane
        .fleet_column(&fleet.id, "config_json")
        .await
        .expect("an installed fleet stores its configuration");
    assert!(
        before.contains("1.0"),
        "the seeded entry declares a 1.0 ceiling: {before}"
    );

    let patched = lane
        .fleets
        .patch(
            &lane.workspace,
            &fleet.id,
            &Patch {
                config: Some(ConfigSource::Trigger(TRIGGER_MD_EDITED.to_owned())),
                ..Patch::default()
            },
            Lane::now(),
        )
        .await
        .expect("a well-formed TRIGGER.md is storable");

    let after = lane
        .fleet_column(&fleet.id, "config_json")
        .await
        .expect("the row survives its own edit");
    assert!(
        after.contains("5.0"),
        "the next lease reads the edited ceiling: {after}"
    );
    assert_ne!(before, after, "the stored configuration actually moved");
    assert!(
        patched.revision > 0,
        "the revision is the caller's handle on which version they now hold"
    );

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Redis"]
async fn a_status_only_patch_leaves_the_configuration_alone() {
    // The other half of 3.2, and the reason the PATCH `COALESCE`s each column:
    // stopping a fleet must not rewrite what it would run when resumed.
    let lane = Lane::create().await;
    let fleet = installed(&lane).await;
    let before = lane.fleet_column(&fleet.id, "config_json").await;

    lane.fleets
        .patch(
            &lane.workspace,
            &fleet.id,
            &Patch {
                status: Some(Requested::Stopped),
                ..Patch::default()
            },
            Lane::now(),
        )
        .await
        .expect("active → stopped is a legal transition");

    assert_eq!(
        lane.fleet_column(&fleet.id, "status").await.as_deref(),
        Some("stopped")
    );
    assert_eq!(
        lane.fleet_column(&fleet.id, "config_json").await,
        before,
        "a lifecycle change is not a configuration change"
    );

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Redis"]
async fn a_fleet_in_another_workspace_is_not_found_rather_than_forbidden() {
    // Dimension 3.3's 404 half. The caller OWNS the workspace they named — the
    // ownership layer would admit them — and the fleet is simply not in it.
    // Every statement is workspace-scoped in its predicate, so this daemon never
    // learns the fleet exists elsewhere and could not disclose it if asked.
    let lane = Lane::create().await;
    let fleet = installed(&lane).await;
    let elsewhere = lane.another_workspace().await;

    for outcome in [
        lane.fleets
            .detail(&elsewhere, &fleet.id)
            .await
            .err()
            .map(|failure| failure.code()),
        lane.fleets
            .patch(
                &elsewhere,
                &fleet.id,
                &Patch {
                    status: Some(Requested::Stopped),
                    ..Patch::default()
                },
                Lane::now(),
            )
            .await
            .err()
            .map(|failure| failure.code()),
        lane.fleets
            .purge(&elsewhere, &fleet.id)
            .await
            .err()
            .map(|failure| failure.code()),
    ] {
        assert_eq!(
            outcome,
            Some(error_code::AGENTSFLEET_NOT_FOUND),
            "a fleet outside the named workspace is invisible, not forbidden"
        );
    }

    // And the row is untouched by any of it.
    assert_eq!(
        lane.fleet_column(&fleet.id, "status").await.as_deref(),
        Some("active")
    );

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Redis"]
async fn two_conditional_writes_race_and_exactly_one_of_them_lands() {
    // The compare-and-set, which is the whole reason the PATCH takes no row
    // lock. Both callers read the same version and both send that version's
    // `If-Match`; the predicate is what decides, inside the UPDATE.
    //
    // A lost update here would be silent — both callers would see 200 and one
    // edit would simply be gone — so the assertion is that exactly one succeeds
    // AND the stored configuration is the winner's.
    let lane = Lane::create().await;
    let fleet = installed(&lane).await;
    let tag = lane
        .fleets
        .detail(&lane.workspace, &fleet.id)
        .await
        .expect("the fleet reads back")
        .etag();

    let conditional = |document: &str| Patch {
        config: Some(ConfigSource::Trigger(document.to_owned())),
        if_match: Some(tag.clone()),
        ..Patch::default()
    };
    // Both documents differ from the STORED one, and that is load-bearing. The
    // predicate hashes the row's own markdown, so a writer resending the stored
    // bytes writes nothing, leaves the hash where it was, and the other writer's
    // guard still matches — both then report success and the assertion below
    // fails on whichever future finished first. An idempotent write is not a
    // version moving; only two real edits make one of them a loser.
    //
    // Bound before the join: both futures borrow their request, and a temporary
    // built inside the macro would be dropped while still held.
    let edited = conditional(TRIGGER_MD_EDITED);
    let rival = conditional(crate::support::TRIGGER_MD_RIVAL);
    let (first, second) = tokio::join!(
        lane.fleets
            .patch(&lane.workspace, &fleet.id, &edited, Lane::now()),
        lane.fleets
            .patch(&lane.workspace, &fleet.id, &rival, Lane::now()),
    );

    let landed = usize::from(first.is_ok()) + usize::from(second.is_ok());
    assert_eq!(landed, 1, "exactly one conditional write may win the race");

    let refused = first
        .err()
        .or(second.err())
        .expect("one of them is refused");
    assert_eq!(
        refused.code(),
        error_code::AGENTSFLEET_SOURCE_STALE,
        "the loser is told its version moved, not that the fleet is gone"
    );
    assert!(
        refused.stale_tag().is_some(),
        "and is handed the tag the row holds now, so it re-applies in one trip"
    );

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Redis"]
async fn a_killed_fleet_is_a_tombstone_and_only_then_purges() {
    // The two-step delete, end to end: a live fleet refuses the purge, and the
    // kill is what makes it eligible. Both refusals come from a predicate, so
    // neither is provable without a row.
    let lane = Lane::create().await;
    let fleet = installed(&lane).await;

    let too_soon = lane
        .fleets
        .purge(&lane.workspace, &fleet.id)
        .await
        .expect_err("a live fleet may not be purged");
    assert_eq!(too_soon.code(), error_code::AGENTSFLEET_ALREADY_TERMINAL);
    assert_eq!(
        lane.fleet_count(&lane.workspace).await,
        1,
        "nothing partial"
    );

    lane.fleets
        .patch(
            &lane.workspace,
            &fleet.id,
            &Patch {
                status: Some(Requested::Killed),
                ..Patch::default()
            },
            Lane::now(),
        )
        .await
        .expect("active → killed is legal");

    let after_death = lane
        .fleets
        .patch(
            &lane.workspace,
            &fleet.id,
            &Patch {
                status: Some(Requested::Active),
                ..Patch::default()
            },
            Lane::now(),
        )
        .await
        .expect_err("a killed fleet is terminal");
    assert_eq!(
        after_death.code(),
        error_code::AGENTSFLEET_NOT_FOUND,
        "a tombstone answers as gone rather than as a refused transition"
    );

    lane.fleets
        .purge(&lane.workspace, &fleet.id)
        .await
        .expect("a killed fleet purges");
    assert_eq!(lane.fleet_count(&lane.workspace).await, 0);

    lane.cleanup().await;
}
