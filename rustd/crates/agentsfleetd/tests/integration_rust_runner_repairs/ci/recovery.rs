//! Lost HTTP acknowledgements never authorize repeating committed GitHub mutations.
use crate::definition_install::install_definition;
use crate::definition_repair::head_reads;
use crate::definition_run::run_event;
use crate::fake_model::FakeModel;
use crate::https::Upstream;
use crate::integration_rust_runner_repairs::LINKWARDEN;
use crate::repair_remote::{Acknowledgement, Readback, Remote};
use crate::repair_walk::{self, BLOCKED};
use afd_gate::policy::repair::branch_for;
use agentsfleetd::supervisor::Supervisor;
use hyper::Method;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn should_reconcile_uncertain_repair_writes() {
    for loss in [Acknowledgement::LoseRef, Acknowledgement::LoseDraft] {
        for readback in [Readback::Exact, Readback::Forbidden, Readback::WrongHead] {
            recover(loss, readback).await;
        }
    }
}

async fn recover(loss: Acknowledgement, readback: Readback) {
    let mut supervisor = Supervisor::new();
    let run = install_definition(&mut supervisor, "ci-repairer", &[], None).await;
    let remote = Remote::new(LINKWARDEN, &branch_for(&run.event_id), loss, readback);
    let upstream = Upstream::serve(remote.routes()).await;
    let (model, transcript) =
        FakeModel::deciding(repair_walk::script(LINKWARDEN, head_reads(LINKWARDEN)));
    let result = run_event(&run, &run.event_id, &upstream, model).await;
    assert_eq!(
        result.answer,
        if readback == Readback::Exact {
            "https://github.com/agentsfleet/linkwarden/pull/9"
        } else {
            BLOCKED
        }
    );
    let state = remote.state();
    assert_eq!(state.ref_posts, 1);
    assert!(state.reference.is_some());
    assert_eq!(state.lost_replies, 1);
    let draft_expected = loss == Acknowledgement::LoseDraft || readback == Readback::Exact;
    assert_eq!(state.draft_posts, usize::from(draft_expected));
    assert_eq!(state.draft.is_some(), draft_expected);
    assert!(transcript.asked().iter().any(|turn| {
        turn.results
            .iter()
            .any(|result| result.starts_with("[upstream_unreachable]"))
    }));
    assert_reconciled_before_any_later_write(&upstream.seen(), loss, readback);
    let trace = crate::integration_tool_trace::stored_trace(&run)
        .await
        .expect("the native event retained its trace");
    assert_eq!(trace["omitted_call_count"], 0);
    assert!(trace.to_string().contains("[upstream_unreachable]"));
    supervisor.shutdown().await;
    run.cleanup().await;
}

fn assert_reconciled_before_any_later_write(
    seen: &[crate::https::Seen],
    loss: Acknowledgement,
    readback: Readback,
) {
    let boundary = LINKWARDEN.path(if loss == Acknowledgement::LoseRef {
        "git/refs"
    } else {
        "pulls"
    });
    let index = seen
        .iter()
        .position(|request| request.method == Method::POST && request.path == boundary)
        .expect("the committed write");
    let later = &seen[index + 1..];
    assert!(later.first().is_some_and(
        |request| request.method == Method::GET && request.path.contains("git/ref/heads/")
    ));
    if readback == Readback::Exact {
        assert!(
            later
                .get(1)
                .is_some_and(|request| request.method == Method::GET
                    && request.path == LINKWARDEN.path("pulls")
                    && request.query.contains("state=all"))
        );
    } else {
        assert!(later.iter().all(|request| request.method == Method::GET));
    }
}
