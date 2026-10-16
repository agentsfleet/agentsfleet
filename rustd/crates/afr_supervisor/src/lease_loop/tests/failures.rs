use super::*;

#[tokio::test(start_paused = true)]
async fn a_sandbox_that_cannot_be_built_refuses_the_lease() {
    let engine = FakeEngine {
        refuse: true,
        ..FakeEngine::default()
    };
    let mut rig = rig(healthy, engine, Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let calls = rig.calls();
    assert_eq!(reported(&calls)[FAILURE_REASON], STARTUP_POSTURE);
    assert_eq!(
        rig.runs.load(Ordering::SeqCst),
        0,
        "nothing runs without a sandbox"
    );
    assert_eq!(position(&calls, Verb::Capture), None);
}

#[tokio::test(start_paused = true)]
async fn an_engine_that_fails_or_panics_reports_a_crash_and_is_torn_down() {
    for behaviour in [Behaviour::Break, Behaviour::Panic] {
        let engine = FakeEngine {
            fail_teardown: true,
            ..FakeEngine::default()
        };
        let mut rig = rig(healthy, engine, behaviour);

        rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

        let settled = (
            reported(&rig.calls())[FAILURE_REASON].clone(),
            rig.destroyed.load(Ordering::SeqCst),
        );
        assert_eq!(settled, (RUNNER_CRASH.into(), 1), "{behaviour:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_lease_that_cannot_start_reports_why() {
    let cases: [(Special, Verb); 2] = [
        (
            |call| {
                (call.verb == Verb::Hydrate)
                    .then(|| Answer::Fail(error::refused(Verb::Hydrate, 403, None)))
            },
            Verb::Hydrate,
        ),
        (
            |call| (call.verb == Verb::Hydrate).then(|| Answer::Reply(Bytes::from_static(b"["))),
            Verb::Hydrate,
        ),
    ];
    for (special, stopped_at) in cases {
        let mut rig = rig(special, FakeEngine::default(), Behaviour::Answer);

        rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

        let calls = rig.calls();
        assert_eq!(reported(&calls)[FAILURE_REASON], STARTUP_POSTURE);
        assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
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
async fn a_lease_arriving_as_the_pool_shuts_down_is_reported_not_run() {
    let mut rig = rig(healthy, FakeEngine::default(), Behaviour::Answer);
    let (turns, coordinator) = FleetTurns::start();
    drop(coordinator);

    rig.lessee
        .run(&turns, &lease(LEASE_ID, FLEET_ID), &Egress::closed())
        .await
        .unwrap();

    assert_eq!(reported(&rig.calls())[FAILURE_REASON], STARTUP_POSTURE);
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_lease_with_an_identifier_out_of_form_is_refused() {
    let rig = rig(healthy, FakeEngine::default(), Behaviour::Answer);

    assert!(rig.run(&lease("nope", FLEET_ID)).await.is_err());
}
