use super::*;

#[tokio::test(start_paused = true)]
async fn renewal_keeps_the_lease_through_a_slow_settle_and_stops_once_answered() {
    let slow_push: Special = |call| {
        (call.verb == Verb::Capture).then(|| {
            Answer::Late(
                RENEWAL_TICK * 3,
                Bytes::from_static(br#"{"stored":1,"skipped":0}"#),
            )
        })
    };
    let mut rig = rig(slow_push, FakeEngine::default(), Behaviour::Answer);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();
    tokio::time::sleep(RENEWAL_TICK * 4).await;

    let calls = rig.calls();
    let renewals: Vec<_> = calls
        .iter()
        .enumerate()
        .filter(|(_, call)| call.verb == Verb::Renew)
        .map(|(at, _)| at)
        .collect();
    let report = position(&calls, Verb::Report).unwrap();
    assert!(
        renewals.len() >= 2,
        "the push outlasted two ticks: {renewals:?}"
    );
    assert!(
        renewals.iter().all(|at| *at < report),
        "no renewal once the report was answered"
    );
}

#[tokio::test(start_paused = true)]
async fn a_4xx_renewal_mid_run_ends_it_and_still_tears_down() {
    let mut rig = rig(renewal_lost, FakeEngine::default(), Behaviour::Hang);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let calls = rig.calls();
    assert_eq!(reported(&calls)[FAILURE_REASON], RENEWAL_TERMINATE);
    assert_eq!(
        rig.destroyed.load(Ordering::SeqCst),
        1,
        "torn down exactly once"
    );
    assert_eq!(
        position(&calls, Verb::Capture),
        None,
        "no fenced push for an ended lease"
    );
}

/// A cut keeps what the run handed back: the daemon bills its tokens and
/// keeps its memory, and the report names the cut as the reason it ended.
#[tokio::test(start_paused = true)]
async fn a_run_cut_by_its_renewal_still_reports_its_tokens_and_pushes_its_memory() {
    let mut rig = rig(renewal_lost, FakeEngine::default(), Behaviour::Stops);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let calls = rig.calls();
    let report = reported(&calls);
    assert_eq!(report[FAILURE_REASON], RENEWAL_TERMINATE);
    assert_eq!(report[TOKENS], 8, "the run's tokens are billed");
    assert_eq!(report[INPUT_TOKENS], 3);
    assert_eq!(report[OUTPUT_TOKENS], 4);
    assert!(
        position(&calls, Verb::Capture).is_some(),
        "the run's memory is pushed"
    );
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 1);
}

/// A run the daemon cut before it answered has only the meter to bill: the
/// renewal that ended it carried its spend, and so does the failed report.
#[tokio::test(start_paused = true)]
async fn a_run_cut_before_it_answered_bills_the_meter_at_renewal_and_in_its_report() {
    let mut rig = rig(renewal_lost, FakeEngine::default(), Behaviour::Spends);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let calls = rig.calls();
    let renewal = position(&calls, Verb::Renew).unwrap();
    let renewed: serde_json::Value =
        serde_json::from_slice(calls[renewal].body.as_ref().unwrap()).unwrap();
    assert_eq!(renewed[INPUT_TOKENS], 3, "the renewal carried the meter");
    assert_eq!(renewed[CACHED_INPUT_TOKENS], 1);
    assert_eq!(renewed[OUTPUT_TOKENS], 4);
    let report = reported(&calls);
    assert_eq!(report[FAILURE_REASON], RENEWAL_TERMINATE);
    assert_eq!(report[TOKENS], 8, "the failed report bills the meter");
    assert_eq!(report[INPUT_TOKENS], 3);
    assert_eq!(report[CACHED_INPUT_TOKENS], 1);
    assert_eq!(report[OUTPUT_TOKENS], 4);
}

#[tokio::test(start_paused = true)]
async fn a_stop_ends_a_lease_in_flight_and_still_tears_down() {
    let mut rig = rig(healthy, FakeEngine::default(), Behaviour::Hang);
    let stopping = async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        rig.lessee.halt.stop();
    };

    let granted = lease(LEASE_ID, FLEET_ID);
    let (ran, ()) = tokio::join!(rig.run(&granted), stopping);

    ran.unwrap();
    let report = reported(&rig.calls());
    assert_eq!(report[FAILURE_REASON], RENEWAL_TERMINATE);
    assert!(
        report["failure_detail"]
            .as_str()
            .unwrap()
            .contains("told to stop")
    );
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_renewal_refused_before_the_fleet_is_free_never_starts_the_run() {
    let mut rig = rig(renewal_lost, FakeEngine::default(), Behaviour::Answer);
    let (turns, coordinator) = FleetTurns::start();
    tokio::spawn(coordinator);
    let _busy = turns.claim(&Uuid7::parse(FLEET_ID).unwrap()).await;

    rig.lessee
        .run(&turns, &lease(LEASE_ID, FLEET_ID), &Egress::closed())
        .await
        .unwrap();

    assert_eq!(reported(&rig.calls())[FAILURE_REASON], RENEWAL_TERMINATE);
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
}
