//! The lease verbs at their edges, against live datastores: a host that may
//! no longer hold work, a lease that already ended, a slice that measured
//! nothing, an event already finished, and a runner with no lease at all.
//!
//! Each edge answers a verdict, never a fault, because the runner acts on the
//! verdict: a lost lease stops the child, nothing to charge charges nothing,
//! and a runner the daemon does not know is told there is no work.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_billing::Nanos;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_crypto::entropy::Entropy;
use afd_db::test_util::mint_uuid7;
use afd_fleet::lease::Renewed;
use afd_fleet::lease::sql::meter::Metering;
use afd_wire::lease::AcceptedKinds;
use afd_wire::report::{Outcome, RenewRequest, ReportRequest};

use crate::integration_held_sandbox::report::processed;
use crate::queue;
use crate::report_seed::{Held, held};

/// A host's degraded verdict, which bars it from claiming and renewing alike.
const DEGRADE_HOST: &str = "UPDATE runner.runners SET degraded_reason = $2 WHERE id = $1::uuid";
const HOST_DEGRADED: &str = "cgroup controller missing";

/// Moves the lease's runtime cursor past the database clock, so the next
/// window measures no run time at all.
const CURSOR_AHEAD: &str = "UPDATE runner.leases \
     SET last_metered_at = now() + interval '1 hour' WHERE id = $1::uuid";

/// Ends the leased event before its report lands, as a redelivery's first
/// report would have.
const FINISH_EVENT: &str = "UPDATE fleet.events SET status = $3 \
     WHERE fleet_id = $1::uuid AND event_id = $2";

/// A finished event's status.
const PROCESSED: &str = "processed";

/// Two usage lines on the lease whose token counts sum past a 64-bit integer,
/// so the post-commit token read fails as a database fault while the money,
/// which reads charges and counts rows, is untouched.
const OVERFLOWING_LINES: &str = "INSERT INTO billing.usage_lines \
       (id, work_type, work_id, seq, tenant_id, workspace_id, fleet_id, fleet_name, \
        event_id, provider, model, posture, requested_at, input_token_count, \
        cached_input_token_count, cache_write_token_count, cache_write_5m_token_count, \
        cache_write_1h_token_count, output_token_count, runtime_ms, unpriced_token_count, \
        charged_nanos, settled_at, created_at, updated_at) \
     SELECT s.id, $1, l.id, s.seq, l.tenant_id, l.workspace_id, l.fleet_id, f.name, $2, \
            l.provider, l.model, l.posture, now(), $3::bigint, 0, 0, 0, 0, 0, 0, 0, 0, \
            now(), now(), now() \
       FROM runner.leases l JOIN fleet.fleets f ON f.id = l.fleet_id \
            CROSS JOIN (VALUES (0, $5::uuid), (1, $6::uuid)) AS s(seq, id) \
      WHERE l.id = $4::uuid";

/// The line a failed token read is logged under.
const TOKENS_UNREAD: &str = "report_run_tokens_unread";

/// The line an answered no-work poll is logged under.
const NO_WORK: &str = "runner_lease_no_work";

async fn execute(held: &Held, statement: &'static str, binds: &[&str]) {
    let mut query = sqlx::query(statement);
    for bind in binds {
        query = query.bind(*bind);
    }
    query
        .execute(
            &mut *held
                .fixtures
                .database
                .acquire()
                .await
                .expect("a connection"),
        )
        .await
        .expect("the fixture statement runs");
}

/// A host the daemon no longer knows, and a host whose verdict went degraded,
/// each lose the renewal; a lease already reported is lost before either is
/// asked.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_renewal_its_host_or_its_lease_no_longer_allows_is_lost() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let lease_id = held.issued.lease_id.as_str();
    let stranger = mint_uuid7();

    let unknown = held
        .leases
        .extend(
            Metering {
                lease_id,
                runner_id: &stranger,
                requests: &[],
            },
            &held.lease_key,
            &held.pricing,
            held.now,
        )
        .await
        .expect("an unknown host is a verdict");
    execute(&held, DEGRADE_HOST, &[held.runner.as_str(), HOST_DEGRADED]).await;
    let degraded = plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await;
    execute(
        &held,
        "UPDATE runner.runners SET degraded_reason = NULL WHERE id = $1::uuid",
        &[held.runner.as_str()],
    )
    .await;
    plane
        .report(&held.runner, &processed(&held, None), held.now)
        .await
        .expect("the healthy host reports");
    let reported = plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await;

    assert!(matches!(unknown, Renewed::Lost), "{unknown:?}");
    let lost = Some(error_code::RUN_LEASE_LOST);
    assert_eq!(degraded.err().map(|error| error.code()), lost);
    assert_eq!(reported.err().map(|error| error.code()), lost);
    drop(plane);
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// A window that measured no run time and carried no request charges
/// nothing, and the renewal still extends the lease.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_renewal_that_measured_nothing_charges_nothing() {
    let held = held().await;
    let lease_id = held.issued.lease_id.as_str();
    execute(&held, CURSOR_AHEAD, &[lease_id]).await;

    let (expires_at, charged) = held
        .fixtures
        .plane()
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await
        .expect("an empty window still renews");

    assert_eq!(charged, Nanos::ZERO);
    assert!(expires_at > held.now, "the deadline moved forward");
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// A failed run reported on an event its redelivery already finished, after
/// a window that measured nothing, settles with nothing charged and nothing
/// announced, releases its hold as failed, and reads its tokens as none when
/// they cannot be summed, saying so rather than failing a settled report.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_failed_report_on_a_finished_event_settles_with_nothing_owed() {
    let held = held().await;
    let lease_id = held.issued.lease_id.as_str();
    execute(&held, CURSOR_AHEAD, &[lease_id]).await;
    execute(
        &held,
        FINISH_EVENT,
        &[&held.fleet, &held.event_id, PROCESSED],
    )
    .await;
    execute(
        &held,
        OVERFLOWING_LINES,
        &[
            afd_billing::sql::work::FLEET_LEASE,
            &held.event_id,
            &i64::MAX.to_string(),
            lease_id,
            mint_uuid7().as_str(),
            mint_uuid7().as_str(),
        ],
    )
    .await;
    let failed = ReportRequest {
        outcome: Outcome::FleetError,
        failure_detail: Cow::Borrowed("the child exited 1"),
        ..processed(&held, None)
    };
    let capture = Capture::install();

    let reconciled = held
        .fixtures
        .plane()
        .report(&held.runner, &failed, held.now)
        .await
        .expect("a settled report is not failed by its metrics");

    let unread = capture.only(TOKENS_UNREAD);
    drop(capture);
    assert_eq!(unread.field("lease_id"), Some(lease_id));
    assert_eq!(
        unread.field("error_code"),
        Some(error_code::INTERNAL_DB_QUERY.as_str())
    );
    assert_eq!(reconciled.charged, Nanos::ZERO);
    let open: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM billing.holds WHERE work_id = $1::uuid AND released_at IS NULL",
    )
    .bind(lease_id)
    .fetch_one(
        &mut *held
            .fixtures
            .database
            .acquire()
            .await
            .expect("a connection"),
    )
    .await
    .expect("the holds count");
    assert_eq!(open, 0, "the failed run's hold is released");
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// A runner holding no lease on the fleet is refused its memory window, and a
/// runner the daemon does not know is told there is no work rather than an
/// error.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_runner_with_no_lease_or_no_host_row_gets_no_work() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let fleet = Uuid7::parse(&held.fleet).expect("a fleet id is a v7 spelling");
    let drafts = afd_authoring::Drafts::new(
        held.fixtures.database.clone(),
        Entropy::new(),
        std::sync::Arc::clone(&plane.biller),
    );
    let stranger = mint_uuid7();

    let window = plane.hydrate(&held.spare, &fleet).await;
    let capture = Capture::install();
    let answer = plane
        .claim(&drafts, &stranger, &[], AcceptedKinds::ALL, false, held.now)
        .await
        .expect("an unknown host is answered, not refused");

    let line = capture.only(NO_WORK);
    drop(capture);
    assert_eq!(
        window.err().map(|error| error.code()),
        Some(error_code::RUN_LEASE_NOT_FOUND)
    );
    assert!(answer.contains("\"lease\":null"), "{answer}");
    assert_eq!(line.field("runner_id"), Some(stranger.as_str()));
    drop(plane);
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}
