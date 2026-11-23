//! A run's context read answers only its own runner, at its own fence.

use af_core::error_code;
use af_wire::context::ContextLoadRequest;

use super::fixture::Running;

impl Running {
    /// The fencing token the fixture's run was issued at.
    async fn fence(&self) -> u64 {
        let fence: i64 = sqlx::query_scalar("SELECT fencing_token FROM thread.runs WHERE id = $1")
            .bind(&self.run)
            .fetch_one(
                &mut *self
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("a connection"),
            )
            .await
            .expect("the run row");
        u64::try_from(fence).expect("a stored fence is positive")
    }
}

// Value: protects=a fleet run reads its thread's context only as its own runner
// at its own fence, and the context it is handed hides the notes when printed;
// fails_when=the runner bind, the fence comparison or the owner mapping is
// dropped, or `Debug` prints the checkpoint; why_new=the context read had no
// refusal case of its own; seam=`RunService::load_context` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_context_read_answers_only_its_runner_at_its_fence() {
    let running = Running::create().await;
    let plane = running.fixtures.plane();
    let fence = running.fence().await;
    let stranger = afd_db::test_util::mint_id();
    let at = |fencing_token| ContextLoadRequest { fencing_token };

    let foreign = plane
        .load_context(&stranger, &running.run, &at(fence), running.now)
        .await
        .expect_err("another runner reads nothing");
    let stale = plane
        .load_context(&running.runner, &running.run, &at(fence + 1), running.now)
        .await
        .expect_err("a token the run does not hold reads nothing");
    let read = plane
        .load_context(&running.runner, &running.run, &at(fence), running.now)
        .await
        .expect("the holder reads its thread's context");

    assert_eq!(foreign.code(), error_code::RUN_NOT_FOUND);
    assert_eq!(stale.code(), error_code::RUN_STALE_FENCING_TOKEN);
    assert_eq!(
        read.scope,
        afd_setup::context::Owner::Fleet(running.fleet.clone()).scope(),
        "a fleet run reads its fleet's thread"
    );
    assert!(read.checkpoint.is_none(), "nothing was saved yet");
    assert_eq!(
        format!("{read:?}"),
        "ThreadContext { checkpoint: false, .. }"
    );
    running.cleanup().await;
}
