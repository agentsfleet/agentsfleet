use super::{
    APPROVER, CODE, Code, DragonflyHarness, Fingerprint, Opening, SessionStatus, approval,
    error_code, now, origin, surface,
};

/// Dimension 1.3 — two dashboards click Approve, and exactly one wins.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_device_flow_state_races_on_approve() {
    let harness = DragonflyHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");

    // A hundred, not two: two callers can miss a window a hundred find, and the
    // repository's contention bar is a hundred or more.
    let attempts = (0..100_u16).map(|_| {
        let sessions = sessions.clone();
        let session_id = opened.session_id.clone();
        tokio::spawn(async move {
            sessions
                .approve(&session_id, &approval(), APPROVER, now())
                .await
        })
    });

    let mut approved = 0_u32;
    let mut conflicted = 0_u32;
    for attempt in futures_util::future::join_all(attempts).await {
        match attempt.expect("task") {
            Ok(()) => approved += 1,
            Err(refusal) => {
                assert_eq!(refusal.code(), error_code::SESSION_ALREADY_APPROVED);
                conflicted += 1;
            }
        }
    }
    assert_eq!(approved, 1, "one approval is recorded, whatever the racing");
    assert_eq!(conflicted, 99);
}

/// Dimension 1.3 — a hundred callers present the right code, and one redeems.
///
/// The others are refused as ALREADY CONSUMED rather than replayed, because
/// each is given a distinct fingerprint: the replay window is for the caller
/// who asked first losing its reply, not for a crowd sharing one answer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_device_flow_state_races_on_verify() {
    let harness = DragonflyHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");
    sessions
        .approve(&opened.session_id, &approval(), APPROVER, now())
        .await
        .expect("approve");

    let attempts = (0..100_u16).map(|index| {
        let sessions = sessions.clone();
        let session_id = opened.session_id.clone();
        let agent = format!("agentsfleet/1.0 ({index})");
        tokio::spawn(async move {
            sessions
                .verify(
                    &session_id,
                    &Code::parse(CODE).expect("digits"),
                    &Fingerprint::of("203.0.113.7", &agent, &session_id),
                    now(),
                )
                .await
        })
    });

    let mut redeemed = 0_u32;
    for attempt in futures_util::future::join_all(attempts).await {
        match attempt.expect("task") {
            Ok(payload) => {
                assert!(!payload.repeated, "a distinct caller is not a replay");
                redeemed += 1;
            }
            Err(refusal) => assert_eq!(refusal.code(), error_code::SESSION_CONSUMED),
        }
    }
    assert_eq!(redeemed, 1, "a device-flow code is redeemed exactly once");
}

/// Dimension 1.3 — the caller who asked first may ask again, and only them.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_device_flow_replays_for_the_original_caller_only() {
    let harness = DragonflyHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");
    sessions
        .approve(&opened.session_id, &approval(), APPROVER, now())
        .await
        .expect("approve");

    let code = Code::parse(CODE).expect("digits");
    let first = origin(&opened.session_id);
    let redeemed = sessions
        .verify(&opened.session_id, &code, &first, now())
        .await
        .expect("verify");
    assert!(!redeemed.repeated);

    // The same request again, inside the window: the same envelope, flagged as
    // a repeat for the audit trail and identical on the wire.
    let again = sessions
        .verify(&opened.session_id, &code, &first, now())
        .await
        .expect("replay");
    assert!(again.repeated);
    assert_eq!(again.ciphertext, redeemed.ciphertext);

    // Anybody else asking is told it is gone.
    let stranger = Fingerprint::of("198.51.100.4", "curl/8", &opened.session_id);
    let refusal = sessions
        .verify(&opened.session_id, &code, &stranger, now())
        .await
        .expect_err("consumed");
    assert_eq!(refusal.code(), error_code::SESSION_CONSUMED);
}

/// Dimension 1.3 — verifying before approval never advances the machine.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_device_flow_verify_before_approve_leaves_it_approvable() {
    let harness = DragonflyHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");
    let code = Code::parse(CODE).expect("digits");
    let fingerprint = origin(&opened.session_id);

    for _ in 0..5 {
        let refusal = sessions
            .verify(&opened.session_id, &code, &fingerprint, now())
            .await
            .expect_err("not approved");
        assert_eq!(refusal.code(), error_code::SESSION_NOT_APPROVED);
    }

    // Five early attempts must not have spent the attempt ceiling: no code was
    // ever checked, so nothing was wrong to count.
    sessions
        .approve(&opened.session_id, &approval(), APPROVER, now())
        .await
        .expect("still approvable");
    sessions
        .verify(&opened.session_id, &code, &fingerprint, now())
        .await
        .expect("redeems normally");
}

/// Dimension 1.3 — the attempt ceiling is terminal, and says so distinctly.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_device_flow_attempt_ceiling_aborts_the_session() {
    let harness = DragonflyHarness::connect().await;
    let sessions = surface(&harness);

    let opened = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");
    sessions
        .approve(&opened.session_id, &approval(), APPROVER, now())
        .await
        .expect("approve");

    let wrong = Code::parse("000000").expect("digits");
    let fingerprint = origin(&opened.session_id);
    // Four rejections, then the fifth trips the ceiling. The count is the
    // store's; what this pins is that the LAST one reads differently from the
    // ones before it, so a command line stops prompting.
    for _ in 0..4 {
        let refusal = sessions
            .verify(&opened.session_id, &wrong, &fingerprint, now())
            .await
            .expect_err("rejected");
        assert_eq!(refusal.code(), error_code::VERIFICATION_FAILED);
    }
    let refusal = sessions
        .verify(&opened.session_id, &wrong, &fingerprint, now())
        .await
        .expect_err("ceiling");
    assert_eq!(refusal.code(), error_code::SESSION_ABORTED);

    // And the right code no longer works, because the session is gone.
    let refusal = sessions
        .verify(
            &opened.session_id,
            &Code::parse(CODE).expect("digits"),
            &fingerprint,
            now(),
        )
        .await
        .expect_err("aborted");
    assert_eq!(refusal.code(), error_code::SESSION_ABORTED);
}

/// Dimension 1.2 — one person's bulk cancel touches nobody else's logins.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_device_flow_bulk_cancel_is_scoped_to_its_owner() {
    let harness = DragonflyHarness::connect().await;
    let sessions = surface(&harness);
    let owner = format!("{APPROVER}_{}", std::process::id());
    let other = format!("{owner}_other");

    let mut mine = Vec::new();
    for _ in 0..3 {
        let opened = sessions
            .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
            .await
            .expect("open");
        sessions
            .approve(&opened.session_id, &approval(), &owner, now())
            .await
            .expect("approve");
        mine.push(opened.session_id);
    }
    let theirs = sessions
        .open(&Opening::parse("cli-key", "laptop").expect("bounds"), now())
        .await
        .expect("open");
    sessions
        .approve(&theirs.session_id, &approval(), &other, now())
        .await
        .expect("approve");

    let aborted = sessions.cancel_all(&owner).await.expect("cancel all");
    for session_id in &mine {
        assert!(
            aborted.contains(session_id),
            "{session_id} belongs to the owner and must be reported aborted"
        );
        let refusal = sessions.poll(session_id).await.expect_err("aborted");
        assert_eq!(refusal.code(), error_code::SESSION_ABORTED);
    }
    assert!(
        !aborted.contains(&theirs.session_id),
        "another person's login must survive a bulk cancel"
    );
    let survivor = sessions.poll(&theirs.session_id).await.expect("still live");
    assert_eq!(survivor.status, SessionStatus::VerificationPending);
}
