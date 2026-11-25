use super::*;

#[tokio::test(start_paused = true)]
async fn an_engine_that_fails_or_panics_reports_a_crash_and_is_torn_down() {
    for behaviour in [Behaviour::Break, Behaviour::Panic] {
        let engine = FakeEngine {
            fail_teardown: true,
            ..FakeEngine::default()
        };
        let mut rig = rig(healthy, engine, behaviour);

        rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

        let settled = (
            reported(&rig.calls())[FAILURE_REASON].clone(),
            rig.destroyed.load(Ordering::SeqCst),
        );
        assert_eq!(settled, (RUNNER_CRASH.into(), 1), "{behaviour:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_run_that_cannot_start_reports_why() {
    let cases: [(Special, Verb); 2] = [
        (
            |call| {
                (call.verb == Verb::Load)
                    .then(|| Answer::Fail(error::refused(Verb::Load, 403, None)))
            },
            Verb::Load,
        ),
        (
            |call| (call.verb == Verb::Load).then(|| Answer::Reply(Bytes::from_static(b"["))),
            Verb::Load,
        ),
    ];
    for (special, stopped_at) in cases {
        let mut rig = rig(special, FakeEngine::default(), Behaviour::Answer);

        rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

        let calls = rig.calls();
        assert_eq!(reported(&calls)[FAILURE_REASON], STARTUP_POSTURE);
        assert_eq!(rig.prepared.load(Ordering::SeqCst), 0, "no build started");
        assert_eq!(rig.runs.load(Ordering::SeqCst), 0, "no model request");
        let setup: Vec<_> = calls
            .iter()
            .map(|call| call.verb)
            .filter(|verb| *verb != Verb::Report)
            .collect();
        assert_eq!(
            setup.last(),
            Some(&stopped_at),
            "it stopped at the step that failed"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_run_arriving_as_the_pool_shuts_down_is_reported_not_run() {
    let mut rig = rig(healthy, FakeEngine::default(), Behaviour::Answer);
    let (turns, coordinator) = FleetTurns::start();
    drop(coordinator);

    crate::test_support::bounded(
        "run arriving during shutdown",
        rig.lessee.run(
            &turns,
            &payload(RUN_ID, FLEET_ID),
            &Egress::closed(),
            tokio_util::sync::CancellationToken::new(),
        ),
    )
    .await
    .unwrap();

    assert_eq!(reported(&rig.calls())[FAILURE_REASON], STARTUP_POSTURE);
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_run_with_an_identifier_out_of_form_is_refused() {
    let rig = rig(healthy, FakeEngine::default(), Behaviour::Answer);

    assert!(rig.run(&payload("nope", FLEET_ID)).await.is_err());
}

/// An API-only run assigned a saved turn it cannot read back is refused
/// before its model starts, and never builds a sandbox.
#[tokio::test(start_paused = true)]
async fn an_api_only_turn_whose_checkpoint_cannot_be_read_never_starts_its_model() {
    let mut rig = rig(
        |call| {
            (call.verb == Verb::TurnCheckpoint).then(|| {
                Answer::Fail(error::refused(
                    Verb::TurnCheckpoint,
                    409,
                    Some(af_core::error_code::RUN_CHECKPOINT_CONFLICT),
                ))
            })
        },
        FakeEngine::default(),
        Behaviour::Answer,
    );
    let mut run = payload(RUN_ID, FLEET_ID);
    run.event.workspace_id = "019f0000-0000-7000-8000-000000000001".into();
    run.policy.tools.clear();
    run.turn_resume = Some(af_wire::run::TurnResume {
        checkpoint_id: af_wire::run::CheckpointOperationId(RUN_ID.to_owned()),
        generation: 1,
        format_version: af_wire::run::TURN_CHECKPOINT_FORMAT,
        scope: af_wire::run::TurnScope {
            workspace_id: af_wire::run::CheckpointOperationId(run.event.workspace_id.to_string()),
            fleet_id: af_wire::run::CheckpointOperationId(run.event.fleet_id.to_string()),
            turn_id: af_wire::run::CheckpointOperationId(run.event.turn_id.to_string()),
            workspace: af_wire::run::TurnWorkspace::ApiOnly,
        },
        manifest_root: "a".repeat(64),
        history_digest: "b".repeat(64),
    });

    rig.run(&run).await.unwrap();

    let calls = rig.calls();
    let report = reported(&calls);
    assert_eq!(report[FAILURE_REASON], STARTUP_POSTURE);
    assert_eq!(
        report["failure_detail"],
        "ordinary turn recovery could not be established"
    );
    assert!(
        calls.iter().any(|call| call.verb == Verb::TurnCheckpoint),
        "the saved turn was asked for"
    );
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0, "the model never ran");
    assert_eq!(
        rig.prepared.load(Ordering::SeqCst),
        0,
        "no sandbox was built"
    );
}
