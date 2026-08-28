//! §1 against a live datastore — what enrolment writes, and what a beat moves.
//!
//! The other half of `afd_api/tests/runner_plane.rs`. That suite proves the
//! GUARD — who is admitted and who is refused — through the production router
//! with no datastore behind it. This one proves the ROWS, which is the half
//! that needs real Postgres and the half Invariant 5 is about.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/fleet_fixtures.rs"]
mod support;

#[path = "support/fleet_queue.rs"]
mod queue;

#[path = "support/fleet_requests.rs"]
mod requests;

use afd_auth::directory::Digest;
use afd_core::clock::UnixMillis;
use afd_runner::Verdict;
use afd_runner::reconcile::REASON_NO_CAPABILITY_REPORT;
use afd_wire::runner::{NetworkPolicy, SandboxTier};

use self::requests::{ENROLLED_AT, enrolment};
use self::support::Fixtures;

/// Dimension 1.3 (second half) — enrolment stores the DIGEST and never the token.
///
/// The property the whole credential design rests on: the value is revealed in
/// one response and the row keeps a hash of it, so a database read — a backup, a
/// replica, an operator with `SELECT` — cannot recover a working credential.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_enrolment_stores_only_the_digest() {
    let fixtures = Fixtures::create().await;
    let request = enrolment(SandboxTier::LandlockFull, NetworkPolicy::AllowAll, 4);

    let enrolled = fixtures
        .runners()
        .register(&request, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect("a well-formed enrolment against a reachable datastore");

    let runner = enrolled.runner_id.as_str().to_owned();
    let token = enrolled.token.expose().to_owned();
    let digest = Digest::of_minted(&token);

    assert_eq!(
        fixtures.rows_with_token_hash(digest.as_str()).await,
        1,
        "the row must be findable by the digest of what the holder presents — \
         if it is not, the token authenticates nothing"
    );
    assert_eq!(
        fixtures.rows_with_token_hash(&token).await,
        0,
        "the token itself must appear in no row"
    );
    assert_eq!(
        fixtures.runner_column(&runner, "token_hash").await,
        Some(digest.as_str().to_owned())
    );
    // The credential is 32 random bytes rendered as hex behind its marker, so
    // anything shorter is a mint that lost entropy on the way out.
    assert_eq!(token.len(), "agt_r".len() + 64);
    assert!(token.starts_with("agt_r"));

    fixtures.cleanup().await;
}

/// The enrolment row carries the assignment AS STORED, with the clamp applied.
///
/// What the operator is echoed and what the host will apply are the same value,
/// which is only true if the clamp lands on the ROW rather than on the reply.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_enrolment_stores_the_clamped_assignment() {
    let fixtures = Fixtures::create().await;
    // Far past the ceiling: a fat-fingered value must not fork unbounded
    // children on a host, which is what the shared bound exists for.
    let request = enrolment(
        SandboxTier::ContainerNested,
        NetworkPolicy::AllowListEgress,
        9_999,
    );

    let enrolled = fixtures
        .runners()
        .register(&request, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect("an out-of-range worker count is clamped, never refused");

    let runner = enrolled.runner_id.as_str().to_owned();
    assert_eq!(enrolled.worker_count, afd_core::limits::MAX_WORKERS);
    assert_eq!(
        fixtures.runner_column(&runner, "worker_count").await,
        Some(afd_core::limits::MAX_WORKERS.to_string()),
        "the reply and the row must agree — two values with a comment saying \
         which is authoritative is the shape this avoids"
    );
    assert_eq!(
        fixtures.runner_column(&runner, "sandbox_tier").await,
        Some("container_nested".to_owned())
    );
    assert_eq!(
        fixtures.runner_column(&runner, "network_policy").await,
        Some("allow_list_egress".to_owned())
    );
    assert_eq!(
        fixtures.runner_column(&runner, "admin_state").await,
        Some("active".to_owned())
    );
    assert_eq!(
        fixtures.runner_column(&runner, "last_seen_at").await,
        Some("0".to_owned()),
        "a minted runner has never connected, so the fleet read must derive \
         `registered` rather than a fabricated `online`"
    );

    fixtures.cleanup().await;
}

/// A cage tier opens DEGRADED, and its enrolment event lands in the same write.
///
/// The fail-closed window: between minting a token and the host's first beat
/// there is no evidence, so the lease gate refuses work rather than assuming
/// the cage exists.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_enrolment_opens_degraded_with_its_audit_row() {
    let fixtures = Fixtures::create().await;
    let request = enrolment(SandboxTier::LandlockFull, NetworkPolicy::AllowAll, 1);

    let enrolled = fixtures
        .runners()
        .register(&request, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect("enrolment must succeed");

    let runner = enrolled.runner_id.as_str().to_owned();
    assert_eq!(
        enrolled.verdict,
        Verdict::Degraded {
            reason: REASON_NO_CAPABILITY_REPORT
        }
    );
    assert_eq!(
        fixtures.runner_column(&runner, "degraded").await,
        Some("true".to_owned())
    );
    assert_eq!(
        fixtures.runner_column(&runner, "degraded_reason").await,
        Some(REASON_NO_CAPABILITY_REPORT.to_owned())
    );
    assert_eq!(
        fixtures.events(&runner).await,
        vec!["runner_registered".to_owned()],
        "the row and the event that explains where it came from land in ONE \
         statement, so an observer can never see one without the other"
    );

    fixtures.cleanup().await;
}

/// The self read answers the row, and does NOT touch liveness.
///
/// `agentsfleet-runner status` must never be able to make a dead host look
/// alive, which is a property of the statement rather than of a handler
/// remembering not to ask.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_the_self_read_never_bumps_liveness() {
    let fixtures = Fixtures::create().await;
    let request = enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 2);
    let enrolled = fixtures
        .runners()
        .register(&request, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect("enrolment must succeed");

    let row = fixtures
        .runners()
        .self_record(&enrolled.runner_id)
        .await
        .expect("the runner reads its own row");

    assert_eq!(row.id, enrolled.runner_id);
    assert_eq!(row.status, "active");
    assert_eq!(row.host_id, "host-01.fixture.test");
    assert_eq!(row.last_seen_at, 0);
    let assigned = row
        .assignment
        .decode()
        .expect("a fully assigned row decodes");
    assert_eq!(assigned.sandbox_tier, SandboxTier::DevNone);
    assert_eq!(assigned.registry_allowlist, ["registry.npmjs.org"]);
    assert_eq!(assigned.worker_count, 2);
    // Read twice; still never seen.
    let runner = enrolled.runner_id.as_str().to_owned();
    assert_eq!(
        fixtures.runner_column(&runner, "last_seen_at").await,
        Some("0".to_owned()),
        "the self read has no update in it, so reading it again cannot move \
         liveness however many times a status command is run"
    );

    fixtures.cleanup().await;
}
