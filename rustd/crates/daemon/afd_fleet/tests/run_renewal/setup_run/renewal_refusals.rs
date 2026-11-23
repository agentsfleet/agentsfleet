//! What a setup run's renewal refuses, and what it still owes: a run that is
//! not this runner's, cancelled, settled, lapsed or on a host that may not
//! renew is refused with the code a fleet's renewal answers, and the request
//! lines the runner reported are logged either way. A payer whose funds gate
//! cannot be read is admitted and said so; one that cannot fund the next
//! slice is refused.

use std::sync::Arc;
use std::time::Duration;

use af_core::error_code::{self, ErrorCode};
use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;
use af_wire::report::RenewRequest;
use af_wire::usage::{LineKind, RequestLines, RequestUsage};
use afd_billing::Accounts;
use afd_billing::ledger::LedgerBiller;
use afd_fleet::run::RunService;
use jiff::Timestamp;
use sqlx::Connection as _;

use super::{Case, processed};
use crate::rows::Row;

/// The input tokens the runner's one reported request read.
pub(super) const INPUT_TOKENS: u32 = 1_200;

/// The event a draft run its payer can no longer fund logs.
const NO_CREDITS: &str = "draft_run_no_credits";

/// The event a renewal admitted past an unreadable funds gate logs.
const GATE_UNREADABLE: &str = "renew_gate_unreadable_admitted";

/// How long a race waits for the renewal to reach the held row.
const WAIT_LIMIT: Duration = Duration::from_secs(5);

/// Whether a backend waits on the one `$1` holds.
const BLOCKED: &str =
    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))";

/// A renewal carrying one request the runner made since its last renewal.
pub(super) fn one_request() -> RenewRequest {
    RenewRequest {
        requests: RequestLines(vec![RequestUsage {
            seq: 0,
            at: af_core::clock::now(),
            input_tokens: INPUT_TOKENS,
            cached_input_tokens: 0,
            cache_write_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
            output_tokens: 300,
        }]),
    }
}

impl Case {
    /// `plane`'s renewal of `run` for the fixture host.
    async fn renew_on(
        &self,
        plane: &RunService,
        run: &Uuid7,
        request: &RenewRequest,
    ) -> Result<Timestamp, afd_fleet::Error> {
        plane
            .renew_draft(&self.running.runner, run, request, af_core::clock::now())
            .await
    }

    /// The fixture plane's renewal of `run`.
    async fn renew(
        &self,
        run: &Uuid7,
        request: &RenewRequest,
    ) -> Result<Timestamp, afd_fleet::Error> {
        self.renew_on(&self.running.fixtures.plane(), run, request)
            .await
    }

    /// The model lines logged against `run`: each line's number and input
    /// tokens.
    pub(super) async fn model_lines(&self, run: &Uuid7) -> Vec<(i64, i64)> {
        sqlx::query_as(
            "SELECT seq, input_token_count FROM billing.usage_lines \
             WHERE work_id = $1 AND kind = $2 ORDER BY seq",
        )
        .bind(run)
        .bind(LineKind::Model.as_str())
        .fetch_all(&mut *self.connection().await)
        .await
        .expect("the run's model lines")
    }

    /// Runs `statement` with `run` as `$1`, expecting it to touch one row.
    pub(super) async fn alter(&self, statement: &'static str, row: &Uuid7) {
        let changed = sqlx::query(statement)
            .bind(row)
            .execute(&mut *self.connection().await)
            .await
            .expect("the fixture row changes");
        assert_eq!(changed.rows_affected(), 1, "{statement}");
    }

    /// The run's deadline as its row holds it.
    async fn deadline(&self, run: &Uuid7) -> Option<Timestamp> {
        self.running
            .fixtures
            .instant(Row::Run(run), "run_expires_at")
            .await
    }

    /// Runs `work` while another transaction holds `run`'s row, running
    /// `statement` on it there once `work` waits on it, then committing.
    pub(super) async fn while_held<T>(
        &self,
        run: &Uuid7,
        statement: &'static str,
        work: impl Future<Output = T>,
    ) -> T {
        let database = &self.running.fixtures.database;
        let mut connection = database.acquire().await.expect("a blocking connection");
        let mut held = connection.begin().await.expect("the blocker opens");
        let blocker: i32 =
            sqlx::query_scalar("SELECT pg_backend_pid() FROM thread.runs WHERE id = $1 FOR UPDATE")
                .bind(run)
                .fetch_one(&mut *held)
                .await
                .expect("the run row is held");
        let change = async {
            tokio::time::timeout(WAIT_LIMIT, async {
                let mut observer = database.acquire().await.expect("an observer");
                while !sqlx::query_scalar::<_, bool>(BLOCKED)
                    .bind(blocker)
                    .fetch_one(&mut *observer)
                    .await
                    .expect("the lock graph reads")
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("the work reaches the held run");
            let changed = sqlx::query(statement)
                .bind(run)
                .execute(&mut *held)
                .await
                .expect("the held run changes");
            assert_eq!(changed.rows_affected(), 1, "{statement}");
            held.commit().await.expect("the blocker commits");
        };
        let (done, ()) = tokio::join!(work, change);
        done
    }

    /// Renews `run` with [`one_request`] while `statement` runs on its held row.
    async fn renew_while(
        &self,
        run: &Uuid7,
        statement: &'static str,
    ) -> Result<Timestamp, afd_fleet::Error> {
        let request = one_request();
        // Boxed: the renewal's state would otherwise ride inside the race's
        // future, past the large-future bound.
        Box::pin(self.while_held(run, statement, Box::pin(self.renew(run, &request)))).await
    }
}

/// The code `answer` was refused with.
pub(super) fn refused<T: std::fmt::Debug>(answer: Result<T, afd_fleet::Error>) -> ErrorCode {
    answer.expect_err("the renewal is refused").code()
}

/// The fixture plane with its accounts and its biller each over a datastore
/// nobody answers on when asked, and over the live one otherwise.
fn plane_with_dead(case: &Case, accounts: bool, biller: bool) -> RunService {
    let fixtures = &case.running.fixtures;
    let dead = afd_db::test_util::unreachable_db;
    RunService {
        accounts: Accounts::new(if accounts {
            dead()
        } else {
            fixtures.database.clone()
        }),
        biller: Arc::new(LedgerBiller::new(if biller {
            dead()
        } else {
            fixtures.database.clone()
        })),
        ..fixtures.plane()
    }
}

// Value: protects=a runner cannot renew a run it does not hold; fails_when=the
// draft run read stops scoping by runner, or a missing row is renewed;
// why_new=the renewal's not-found arm had no case; seam=renew_draft over live
// Postgres, naming a run nobody issued.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_setup_renewal_naming_no_run_of_this_runner_is_not_found() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let before = case.deadline(&taken.run_id()).await;

    let answer = case
        .renew(&afd_db::test_util::mint_id(), &one_request())
        .await;

    assert_eq!(refused(answer), error_code::RUN_NOT_FOUND);
    assert_eq!(
        case.deadline(&taken.run_id()).await,
        before,
        "the held run is untouched"
    );
    case.cleanup().await;
}

// Value: protects=a person's Stop ends a setup run at its next renewal, and the
// usage the runner reported is still billed; fails_when=the cancel mark is not
// read before the gates, the run is extended, or the refused renewal drops its
// request lines; why_new=the cancelled arm and the refused renewal's lines had no
// case; seam=renew_draft over live Postgres, the run's cancel mark set.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_cancelled_setup_run_is_refused_and_its_request_lines_are_logged() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let run = taken.run_id();
    case.alter(
        "UPDATE thread.runs SET cancel_requested_at = now() WHERE id = $1",
        &run,
    )
    .await;
    let before = case.deadline(&run).await;

    let answer = case.renew(&run, &one_request()).await;

    assert_eq!(refused(answer), error_code::RUN_CANCELLED);
    assert_eq!(case.deadline(&run).await, before, "the run is not extended");
    assert_eq!(
        case.model_lines(&run).await,
        [(0, i64::from(INPUT_TOKENS))],
        "the request the runner spent is owed"
    );
    case.cleanup().await;
}

// Value: protects=a settled setup run is never renewed back to life;
// fails_when=the status check is dropped and a reported run extends; why_new=the
// not-active arm had no case; seam=report_draft, then renew_draft over live
// Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_reported_setup_runs_renewal_is_lost() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    case.report(&processed(&taken, None))
        .await
        .expect("the run settles");
    let before = case.deadline(&taken.run_id()).await;

    let answer = case.renew(&taken.run_id(), &RenewRequest::default()).await;

    assert_eq!(refused(answer), error_code::RUN_LOST);
    assert_eq!(
        case.deadline(&taken.run_id()).await,
        before,
        "the settled run keeps its settled deadline"
    );
    assert_eq!(
        case.runs().await,
        [(1, afd_state::sql::RUN_STATUS_REPORTED.to_owned())]
    );
    case.cleanup().await;
}

// Value: protects=a host the platform degraded stops renewing its setup run, and
// the run's reported usage is billed; fails_when=the host's renew check is
// dropped from the draft path, or its refusal skips the request lines;
// why_new=the draft host refusal had no case; seam=renew_draft over live Postgres,
// the host's degraded verdict set.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_setup_run_on_a_degraded_host_is_lost_and_its_request_lines_are_logged() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let run = taken.run_id();
    case.alter(
        "UPDATE runner.runners SET degraded_reason = 'cgroup controller missing' WHERE id = $1",
        &case.running.runner,
    )
    .await;
    let before = case.deadline(&run).await;

    let answer = case.renew(&run, &one_request()).await;

    assert_eq!(refused(answer), error_code::RUN_LOST);
    assert_eq!(case.deadline(&run).await, before, "the run is not extended");
    assert_eq!(case.model_lines(&run).await, [(0, i64::from(INPUT_TOKENS))]);
    case.cleanup().await;
}

// Value: protects=a setup run past its deadline is not revived by a late renewal
// before the sweep expires it; fails_when=the extension's deadline guard is
// dropped, or a guard that moved nothing answers extended; why_new=the unmoved
// extension arm had no draft case; seam=renew_draft over live Postgres, the run and
// its thread lapsed but still active.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_lapsed_setup_run_still_marked_active_is_lost_at_renewal() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let run = taken.run_id();
    case.lapse(&run).await;
    let before = case.deadline(&run).await;

    let answer = case.renew(&run, &one_request()).await;

    assert_eq!(refused(answer), error_code::RUN_LOST);
    assert_eq!(case.deadline(&run).await, before, "the lapse stands");
    assert_eq!(case.model_lines(&run).await, [(0, i64::from(INPUT_TOKENS))]);
    case.cleanup().await;
}

// Value: protects=a Stop committed while a renewal waits for the run's lock is the
// answer the runner hears; fails_when=the renewal trusts the cancel mark it read
// before the lock; why_new=the locked cancel re-read had no draft case; seam=
// renew_draft over live Postgres, the run row held while the mark is set.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_stop_committed_while_a_setup_renewal_waits_is_answered_cancelled() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let run = taken.run_id();
    let before = case.deadline(&run).await;

    let answer = case
        .renew_while(
            &run,
            "UPDATE thread.runs SET cancel_requested_at = now() WHERE id = $1",
        )
        .await;

    assert_eq!(refused(answer), error_code::RUN_CANCELLED);
    assert_eq!(case.deadline(&run).await, before, "the run is not extended");
    assert_eq!(case.model_lines(&run).await, [(0, i64::from(INPUT_TOKENS))]);
    case.cleanup().await;
}

// Value: protects=a run removed while its renewal waits is lost, not extended
// and not an error; fails_when=the locked re-read's missing row is treated as
// live or as a fault; why_new=the vanished-run arm had no case; seam=renew_draft
// over live Postgres, the run row deleted while held, as a runner's removal
// cascades it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_setup_run_removed_while_its_renewal_waits_is_lost() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let run = taken.run_id();

    let answer = case
        .renew_while(&run, "DELETE FROM thread.runs WHERE id = $1")
        .await;

    assert_eq!(refused(answer), error_code::RUN_LOST);
    assert_eq!(case.runs().await, Vec::new(), "the run stays removed");
    assert_eq!(
        case.model_lines(&run).await,
        [(0, i64::from(INPUT_TOKENS))],
        "the usage the runner spent is owed whoever settles"
    );
    case.cleanup().await;
}

// Value: protects=a payer whose balance went into debt mid-turn stops paying for
// more setup time, and an operator can see why; fails_when=the renewal's funds check
// is dropped, admits a short payer, or logs without its code and ids; why_new=the
// short-funds arm had no case; seam=renew_draft over live Postgres, the wallet
// overdrawn after assignment, short of even the zero floor a run on the tenant's
// own key asks.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_setup_run_whose_payer_runs_dry_is_refused_at_renewal() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let run = taken.run_id();
    case.alter(
        "UPDATE billing.wallets SET balance_nanos = -1 WHERE tenant_id = $1",
        &case.seat.tenant,
    )
    .await;
    let before = case.deadline(&run).await;

    let captured = Capture::install();
    let answer = case.renew(&run, &one_request()).await;
    let short = captured.only(NO_CREDITS);
    drop(captured);

    assert_eq!(refused(answer), error_code::RUN_RENEWAL_NO_CREDITS);
    assert_eq!(short.level, tracing::Level::WARN);
    assert_eq!(
        short.field("error_code"),
        Some(error_code::RUN_RENEWAL_NO_CREDITS.as_str())
    );
    assert_eq!(short.field("draft_id"), Some(case.draft.id.as_str()));
    assert_eq!(short.field("run_id"), Some(taken.run.as_str()));
    assert_eq!(case.deadline(&run).await, before, "the run is not extended");
    assert_eq!(case.model_lines(&run).await, [(0, i64::from(INPUT_TOKENS))]);
    case.cleanup().await;
}

// Value: protects=a payer with no wallet row is admitted, as a fleet's is;
// fails_when=a missing wallet refuses the setup run; why_new=the wallet-missing
// arm had no draft case; seam=renew_draft over live Postgres, the tenant's wallet
// removed after assignment.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_setup_run_whose_payer_has_no_wallet_renews() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let run = taken.run_id();
    case.alter(
        "DELETE FROM billing.wallets WHERE tenant_id = $1",
        &case.seat.tenant,
    )
    .await;

    let renewed = case
        .renew(&run, &RenewRequest::default())
        .await
        .expect("a payer with no wallet is admitted");

    assert_eq!(case.deadline(&run).await, Some(renewed));
    case.cleanup().await;
}

// Value: protects=a price or balance read that fails admits one more slice and
// says which gate could not be read; fails_when=an unreadable gate refuses the
// run, or the event drops its gate, code or ids; why_new=both unreadable-gate arms
// had no case; seam=renew_draft over live Postgres, with the balance read and then
// the price read over a datastore nobody answers on. A run on the tenant's own key
// reads no price, so the price case runs on the platform's.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_setup_renewal_whose_funds_gate_cannot_be_read_is_admitted_and_logged() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let run = taken.run_id();

    for (gate, plane) in [
        ("balance", plane_with_dead(&case, false, true)),
        ("price", plane_with_dead(&case, true, false)),
    ] {
        if gate == "price" {
            let moved = sqlx::query("UPDATE thread.runs SET posture = $2 WHERE id = $1")
                .bind(&run)
                .bind(afd_billing::Posture::Platform.as_str())
                .execute(&mut *case.connection().await)
                .await
                .expect("the run moves to the platform's key");
            assert_eq!(moved.rows_affected(), 1);
        }
        let captured = Capture::install();
        let answer = case.renew_on(&plane, &run, &RenewRequest::default()).await;
        let admitted = captured.only(GATE_UNREADABLE);
        drop(captured);

        let renewed = answer.expect("an unreadable gate admits the renewal");
        assert_eq!(case.deadline(&run).await, Some(renewed), "{gate}");
        assert_eq!(admitted.level, tracing::Level::WARN, "{gate}");
        assert_eq!(admitted.field("gate"), Some(gate));
        assert_eq!(admitted.field("draft_id"), Some(case.draft.id.as_str()));
        assert_eq!(admitted.field("run_id"), Some(taken.run.as_str()));
        assert!(
            admitted
                .field("error_code")
                .is_some_and(|code| code.starts_with("UZ-")),
            "{gate}: {admitted:?}"
        );
    }
    case.cleanup().await;
}
