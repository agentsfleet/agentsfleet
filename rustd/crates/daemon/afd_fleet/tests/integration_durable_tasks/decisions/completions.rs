//! A verified workflow completion is retained once, for the scope it was
//! verified in, and only when it is a well-formed signal for a live task.

use afd_fleet::durable_task::{
    CompletionScope, DurableTaskLifecycle as _, DurableTaskScope, VerifiedCompletion,
};

use super::super::fixture::{Case, exercise};
use super::fixture::Decisions;

/// A completion for the bound goal's pull request: installation 1, repository
/// 101, the head the fixture binds.
fn completion(scope: CompletionScope) -> VerifiedCompletion {
    VerifiedCompletion {
        scope,
        github_installation_id: 1,
        github_repository_id: 101,
        head: "0".repeat(40),
        github_workflow_id: 7,
        github_run_id: 9001,
        run_number: 12,
        run_attempt: 1,
    }
}

async fn retained(case: &Case) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM fleet.durable_task_completions WHERE workspace_id=$1")
        .bind(&case.scope.workspace_id)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("completion observer"),
        )
        .await
        .expect("retained completions")
}

// Value: protects=a fleet hook's completion is retained once for its own fleet
// and never for another; fails_when=the fleet scope is dropped from the insert,
// or a redelivery fans out again; why_new=the fleet-scoped arm of completion
// retention had no case; seam=the production recorder over live Postgres, the
// bound goal's pull request as the target.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_scoped_completion_is_retained_once_for_its_own_fleet() {
    exercise(async |case| {
        let decisions = Decisions::create(case).await;
        decisions.bind_goal().await;
        let foreign = DurableTaskScope {
            workspace_id: case.scope.workspace_id.clone(),
            fleet_id: afd_db::test_util::mint_id(),
        };
        let elsewhere = decisions
            .tasks
            .record_github_completion(&completion(CompletionScope::Fleet(foreign)))
            .await
            .expect("another fleet's hook is answered");
        assert_eq!(elsewhere, 0, "no task of that fleet waits on it");

        let own = completion(CompletionScope::Fleet(case.scope.clone()));
        let first = decisions
            .tasks
            .record_github_completion(&own)
            .await
            .expect("the fleet's own completion is retained");
        let again = decisions
            .tasks
            .record_github_completion(&own)
            .await
            .expect("a redelivery is answered");
        assert_eq!((first, again), (1, 0), "retained once");
        assert_eq!(retained(case).await, 1);
        decisions
            .model
            .finish()
            .await
            .expect("model fixture cleanup");
    })
    .await;
}

// Value: protects=a completion outside the signed payload's bounds is ignored,
// never stored or matched; fails_when=the bounds check is dropped and a zero
// identity or a malformed head reaches the completion table; why_new=the
// out-of-bounds arm of completion retention had no case; seam=the production
// recorder over live Postgres, beside a live task the payload would match.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_completion_outside_its_bounds_is_ignored_and_stores_nothing() {
    exercise(async |case| {
        let decisions = Decisions::create(case).await;
        decisions.bind_goal().await;
        let scope = || CompletionScope::Workspace(case.scope.workspace_id.clone());
        let unsigned_head = VerifiedCompletion {
            head: "Z".repeat(40),
            ..completion(scope())
        };
        let zero_installation = VerifiedCompletion {
            github_installation_id: 0,
            ..completion(scope())
        };
        let past_attempts = VerifiedCompletion {
            run_attempt: u32::MAX,
            ..completion(scope())
        };
        for ignored in [unsigned_head, zero_installation, past_attempts] {
            let inserted = decisions
                .tasks
                .record_github_completion(&ignored)
                .await
                .expect("an out-of-bounds payload is answered, not raised");
            assert_eq!(inserted, 0, "{ignored:?}");
        }
        assert_eq!(retained(case).await, 0, "nothing is stored");
        decisions
            .model
            .finish()
            .await
            .expect("model fixture cleanup");
    })
    .await;
}
