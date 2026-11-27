//! When a run neither takes the fleet's park nor leaves one: the daemon did
//! not ask it to resume, the runner takes no new run, or the sandbox died.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;

use af_wire::credentials::MintCredentialResponse;
use af_wire::policy::{RepositoryAccess, RepositoryBinding};
use af_wire::run::RunPayload;
use afr_tools::catalog::GIT;

use af_core::test_util::trace::Capture;
use afr_telemetry::labels::SandboxPark;

use super::super::EVENT_COMPLETED;
use super::revive_tests::parks_counted;
use super::tests::{
    NEXT_RUN_ID, healthy, parking_rig, parks_nothing, released, releases, resumed, settled,
};
use crate::client::{Call, Verb};
use crate::parks::Release;
use crate::test_support::{
    Answer, Behaviour, FIXTURE_BRANCH, FLEET_ID, FakeEngine, GRANTED_UNTIL, OUTCOME, PROCESSED,
    Parking, RUN_ID, Rig, json, payload, reported, repository,
};

/// What the engine counts for a sandbox destroyed rather than parked, as
/// (prepared, parked, resumed, destroyed).
const DESTROYED_UNPARKED: (usize, usize, usize, usize) = (1, 0, 0, 1);
/// A host the first run's sandbox was not built to reach.
const CHANGED_HOST: &str = "api.github.com";
/// The run line's sandbox parts a parked sandbox does and does not reach.
const PREPARE_MS: &str = "prepare_ms";
const CHECKOUT_MS: &str = "checkout_ms";
const REPOSITORY: &str = "acme/widget";
const WORKSPACE_ID: &str = "01890a5d-ac96-774b-bcce-b302099a805a";

/// `payload`, offering `git` and bound to [`REPOSITORY`] in one workspace.
fn bound(mut payload: RunPayload<'static>) -> RunPayload<'static> {
    payload.event.workspace_id = WORKSPACE_ID.into();
    payload.policy.tools = vec![GIT.name().into()];
    payload.policy.repository_binding = Some(RepositoryBinding {
        repositories: vec![REPOSITORY.into()],
        access: RepositoryAccess::Write,
        base_branch: FIXTURE_BRANCH.into(),
    });
    payload
}

/// The daemon's mint, answered with a token valid until the fixture's grant.
fn minted(call: &Call) -> Option<Answer> {
    (call.verb == Verb::Mint).then(|| {
        json(&MintCredentialResponse {
            token: "ghs_heldTokenNeverInTheWorkspace".into(),
            expires_at: GRANTED_UNTIL,
        })
    })
}

/// The daemon does not name this runner's park the fleet's latest sandbox:
/// another runner ran the fleet since, or the event is a redelivery whose
/// first attempt the park may carry. The run builds fresh, and the stale
/// park ends as superseded rather than serving it.
#[tokio::test(start_paused = true)]
async fn test_a_run_not_resuming_builds_fresh_and_ends_the_park() {
    let capture = Capture::install();
    let (rig, counted) = parking_rig(healthy, FakeEngine::default(), Behaviour::Answer);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
    rig.run(&payload(NEXT_RUN_ID, FLEET_ID)).await.unwrap();
    settled(&rig).await;

    assert_eq!(
        counted.read(),
        (2, 2, 0, 2),
        "built fresh, never resumed; the stale park, then shutdown's"
    );
    assert_eq!(
        releases(&capture),
        [
            released(FLEET_ID, Release::Superseded),
            released(FLEET_ID, Release::Shutdown)
        ]
    );
}

/// The park a run does not resume is counted as superseded, never reused.
#[tokio::test(start_paused = true)]
async fn test_a_park_not_resumed_is_counted_superseded() {
    let next = payload(NEXT_RUN_ID, FLEET_ID);

    let counted = parks_counted(Parking::Works, next).await;

    assert_eq!(
        counted,
        [
            SandboxPark::Parked,
            SandboxPark::Superseded,
            SandboxPark::Parked,
            SandboxPark::Shutdown
        ]
    );
}

/// The daemon says resume, and the run takes the park as before.
#[tokio::test(start_paused = true)]
async fn test_a_run_resuming_takes_the_park() {
    let (rig, counted) = parking_rig(healthy, FakeEngine::default(), Behaviour::Answer);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
    rig.run(&resumed(NEXT_RUN_ID, FLEET_ID)).await.unwrap();

    assert_eq!(
        counted.read(),
        (1, 2, 1, 0),
        "one sandbox, resumed and parked"
    );
}

/// A run that takes the fleet's park resumes it and checks nothing out: its
/// line carries the resume as the prepare part and no checkout part, while the
/// run that built the sandbox carries both. Both are bound to a repository,
/// so the parked one skips a checkout it would otherwise make.
#[tokio::test(start_paused = true)]
async fn test_a_resumed_run_logs_no_checkout() {
    let capture = Capture::install();
    let workspace = tempfile::tempdir().unwrap();
    let engine = FakeEngine {
        workspace: Some(workspace.path().to_owned()),
        ..FakeEngine::default()
    };
    let (rig, _counted) = parking_rig(minted, engine, Behaviour::Answer);
    repository(&rig.origins().join(format!("{REPOSITORY}.git")));

    rig.run(&bound(payload(RUN_ID, FLEET_ID))).await.unwrap();
    rig.run(&bound(resumed(NEXT_RUN_ID, FLEET_ID)))
        .await
        .unwrap();

    let completed = |run_id: &str| {
        capture
            .events()
            .into_iter()
            .find(|event| {
                event.field("event") == Some(EVENT_COMPLETED)
                    && event.field("run_id") == Some(run_id)
            })
            .unwrap()
    };
    let built = completed(RUN_ID);
    assert!(built.field(PREPARE_MS).is_some(), "{:?}", built.fields);
    assert!(built.field(CHECKOUT_MS).is_some(), "{:?}", built.fields);
    let reused = completed(NEXT_RUN_ID);
    assert!(reused.field(PREPARE_MS).is_some(), "{:?}", reused.fields);
    assert_eq!(reused.field(CHECKOUT_MS), None, "{:?}", reused.fields);
}

/// The fleet's next run is told to resume, but its policy reaches a host
/// the parked sandbox was not built for: the park ends as a mismatch, never
/// serving it, and the run builds fresh.
#[tokio::test(start_paused = true)]
async fn test_a_run_under_a_changed_policy_builds_fresh_and_ends_the_park() {
    let capture = Capture::install();
    let (rig, counted) = parking_rig(healthy, FakeEngine::default(), Behaviour::Answer);
    let mut next = resumed(NEXT_RUN_ID, FLEET_ID);
    let reached = Cow::Borrowed(CHANGED_HOST);
    next.policy.network_policy.allow.push(reached);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();
    rig.run(&next).await.unwrap();
    settled(&rig).await;

    assert_eq!(
        counted.read(),
        (2, 2, 0, 2),
        "built fresh, never resumed; the mismatched park, then shutdown's"
    );
    assert_eq!(
        releases(&capture),
        [
            released(FLEET_ID, Release::Mismatch),
            released(FLEET_ID, Release::Shutdown)
        ]
    );
}

fn stop_leasing(rig: &Rig) {
    rig.lessee.halt.taking_runs().cancel();
}

fn shut_down(rig: &Rig) {
    rig.shutdown.cancel();
}

/// A runner that takes no new run parks nothing, since no run could take
/// it: a run that ends processed after running stopped, or while the runner
/// shuts down, destroys its sandbox and reports nothing parked.
#[tokio::test(start_paused = true)]
async fn test_a_runner_taking_no_new_run_parks_nothing() {
    for stop in [stop_leasing as fn(&Rig), shut_down] {
        let (mut rig, counted) = parking_rig(healthy, FakeEngine::default(), Behaviour::Answer);
        stop(&rig);

        rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

        let report = reported(&rig.calls());
        assert_eq!(report[OUTCOME], PROCESSED);
        parks_nothing(&report);
        assert_eq!(counted.read(), DESTROYED_UNPARKED);
    }
}

/// A keeper that refuses the park, here one already shut down, hands the
/// parked sandbox back unfiled: the run destroys it at the release and
/// reports nothing parked, rather than leave it with no owner.
#[tokio::test(start_paused = true)]
async fn test_a_park_the_keeper_refuses_is_destroyed_not_parked() {
    let capture = Capture::install();
    let (mut rig, counted) = parking_rig(healthy, FakeEngine::default(), Behaviour::Answer);
    rig.lessee.parks.shutdown().await;

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

    let report = reported(&rig.calls());
    assert_eq!(report[OUTCOME], PROCESSED);
    parks_nothing(&report);
    assert_eq!(
        counted.read(),
        (1, 1, 0, 1),
        "parked, then destroyed at the release"
    );
    assert_eq!(releases(&capture), [released(FLEET_ID, Release::Shutdown)]);
}

/// A sandbox whose processes ended after its run is destroyed, never parked:
/// there is nothing in it for the fleet's next run to continue.
#[tokio::test(start_paused = true)]
async fn test_a_sandbox_no_longer_running_is_destroyed_not_parked() {
    let engine = FakeEngine {
        parking: Parking::Dead,
        ..FakeEngine::default()
    };
    let (mut rig, counted) = parking_rig(healthy, engine, Behaviour::Answer);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

    let report = reported(&rig.calls());
    assert_eq!(report[OUTCOME], PROCESSED);
    parks_nothing(&report);
    assert_eq!(counted.read(), DESTROYED_UNPARKED);
}
