//! A pull request the human did not name, or one outside the fleet's
//! repositories, is refused before any GitHub read.

use super::{
    Case, Decisions, DurableTaskBindingItem, DurableTaskBindingsInput, DurableTaskRequestedTarget,
    checkpoint_server, error_code, exercise, github_authority, github_server, proposed, sha256_hex,
    storage, valid,
};
use afd_fleet::durable_task::DurableTaskLifecycle as _;

fn retargeted(
    input: &DurableTaskBindingsInput,
    repository: &str,
    number: u32,
) -> DurableTaskBindingsInput {
    let mut input = input.clone();
    if let Some(DurableTaskBindingItem::NewWork { target, .. }) = input.items.first_mut() {
        *target = DurableTaskRequestedTarget::ExistingPr {
            repository: repository.to_owned(),
            number,
        };
    }
    let digest =
        sha256_hex(&serde_json::to_vec(&input.interpretation()).expect("exact interpretation"));
    input.plan_digest = digest;
    input
}

// Value: protects=writable work binds only the pull request the human linked, in
// a repository the fleet binds; fails_when=the linked-URL check or the bound
// repository check is dropped from target preflight; why_new=both refusals had
// no case; seam=the production interpret verb over live Postgres, real storage
// and a GitHub origin that must see no request; no_claim=the remote checks.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_pull_request_the_human_did_not_name_or_the_fleet_does_not_bind_is_refused() {
    exercise(async |case: &Case| {
        let mut fixture = Decisions::create(case).await;
        let objects = checkpoint_server::Server::start().await;
        storage::configure(&mut fixture, &objects).await;
        let remote = github_server::Server::start(vec![]).await;
        let tasks = github_authority::publisher(&fixture, &remote).await;
        let input = proposed(&fixture).await;
        let before = case.counts().await;
        for (repository, number) in [("example/linkwarden", 102), ("example/other", 101)] {
            let refused = tasks
                .interpret(case.run(), &valid(retargeted(&input, repository, number)))
                .await
                .expect_err("the target is not the human's bound pull request");
            assert_eq!(
                refused.code(),
                error_code::DURABLE_TASK_INVALID_TARGET,
                "{repository}#{number}"
            );
            assert_eq!(case.counts().await, before, "no task was created");
        }
        objects.finish().await;
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}
