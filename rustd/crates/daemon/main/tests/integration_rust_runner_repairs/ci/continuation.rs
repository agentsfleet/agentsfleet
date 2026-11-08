//! Failed validation corrects one draft; uncertain updates reconcile before retry.
use crate::definition_install::install_definition;
use crate::definition_repair::{HEAD_SHA, NEW_COMMIT, NEW_TREE, head_reads};
use crate::definition_run::run_event;
use crate::fake_model::{FakeModel, Transcript};
use crate::https::{Seen, Upstream};
use crate::integration_rust_runner_repairs::LINKWARDEN;
use crate::repair_remote::{
    Acknowledgement, CORRECTED_RUN, NEXT_BLOB, NEXT_COMMIT, Readback, Remote, State,
    VALIDATION_JOB, VALIDATION_RUN, draft_url,
};
use crate::repair_walk::BLOCKED;
use afd_gate::policy::repair::branch_for;
use agentsfleetd::supervisor::Supervisor;
use hyper::Method;
use serde_json::{Value, json};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn should_correct_failed_validation_in_the_same_draft() {
    for (acknowledgement, readback) in [
        (Acknowledgement::Delivered, Readback::Exact),
        (Acknowledgement::LoseUpdate, Readback::Exact),
        (Acknowledgement::LoseBeforeUpdate, Readback::Exact),
        (Acknowledgement::RejectUpdate, Readback::Exact),
        (Acknowledgement::LoseUpdate, Readback::Forbidden),
        (Acknowledgement::LoseUpdate, Readback::WrongHead),
        (Acknowledgement::Delivered, Readback::ValidationForbidden),
        (Acknowledgement::Delivered, Readback::StaleValidation),
    ] {
        continue_repair(acknowledgement, readback).await;
    }
}

async fn continue_repair(acknowledgement: Acknowledgement, readback: Readback) {
    let mut supervisor = Supervisor::new();
    let run = install_definition(&mut supervisor, "ci-repairer", &[], None).await;
    let remote = Remote::new(
        LINKWARDEN,
        &branch_for(&run.event_id),
        acknowledgement,
        readback,
    );
    let upstream = Upstream::serve(remote.routes()).await;
    let (model, transcript) = FakeModel::deciding(crate::repair_continue::script(
        LINKWARDEN,
        head_reads(LINKWARDEN),
    ));
    let settled = run_event(&run, &run.event_id, &upstream, model).await;
    let succeeded = readback == Readback::Exact && acknowledgement != Acknowledgement::RejectUpdate;
    assert_eq!(
        settled.answer,
        if succeeded {
            draft_url(LINKWARDEN)
        } else {
            BLOCKED.to_owned()
        }
    );
    let state = remote.state();
    let lost = matches!(
        acknowledgement,
        Acknowledgement::LoseUpdate | Acknowledgement::LoseBeforeUpdate
    );
    assert_lost_reply(&state, &transcript, lost);
    assert_candidate(&state, acknowledgement);
    assert_update_walk(&upstream.seen(), succeeded, readback);
    let trace = crate::integration_tool_trace::stored_trace(&run)
        .await
        .expect("the native event trace");
    assert_eq!(trace["omitted_call_count"], 0);
    assert!(trace.to_string().contains("PATCH"));
    if lost {
        assert!(trace.to_string().contains("upstream_unreachable"));
    }
    supervisor.shutdown().await;
    run.cleanup().await;
}

fn assert_lost_reply(state: &State, transcript: &Transcript, lost: bool) {
    assert_eq!(state.lost_replies, usize::from(lost));
    assert_eq!(
        transcript.asked().iter().any(|asked| asked
            .results
            .iter()
            .any(|result| result.starts_with("[upstream_unreachable]"))),
        lost
    );
}

fn assert_candidate(state: &State, acknowledgement: Acknowledgement) {
    assert_eq!((state.ref_posts, state.draft_posts), (1, 1));
    assert_eq!(state.commit_parents, [HEAD_SHA, NEW_COMMIT]);
    assert_eq!(
        state.ref_updates,
        if acknowledgement == Acknowledgement::LoseBeforeUpdate {
            2
        } else {
            1
        }
    );
    assert_eq!(
        state.reference.as_deref(),
        Some(if acknowledgement == Acknowledgement::RejectUpdate {
            NEW_COMMIT
        } else {
            NEXT_COMMIT
        })
    );
    assert_eq!(
        state.blob_contents,
        ["export const retries = 3;\n", "export const retries = 4;\n"]
    );
    let corrected_tree = state.trees.get(1).expect("the correction tree");
    assert_eq!(corrected_tree["base_tree"], NEW_TREE);
    assert_eq!(
        corrected_tree.pointer("/tree/0/sha"),
        Some(&json!(NEXT_BLOB))
    );
}

fn assert_update_walk(seen: &[Seen], succeeded: bool, readback: Readback) {
    let patch = seen
        .iter()
        .position(|request| request.method == Method::PATCH)
        .expect("the event-owned update");
    let failed_check = LINKWARDEN.path(&format!("commits/{NEW_COMMIT}/check-runs"));
    let failed_run = LINKWARDEN.path(&format!("actions/runs/{VALIDATION_RUN}"));
    let failed_jobs = LINKWARDEN.path(&format!("actions/runs/{VALIDATION_RUN}/attempts/1/jobs"));
    let failed_job = LINKWARDEN.path(&format!("actions/jobs/{VALIDATION_JOB}"));
    let failed_log = LINKWARDEN.path(&format!("actions/jobs/{VALIDATION_JOB}/logs"));
    for path in [
        failed_check,
        failed_run,
        failed_jobs,
        failed_job,
        failed_log,
    ] {
        assert!(
            seen[..patch]
                .iter()
                .any(|request| request.method == Method::GET && request.path == path)
        );
    }
    assert_patch_reconciliation(seen, patch);
    let after_check = LINKWARDEN.path(&format!("commits/{NEXT_COMMIT}/check-runs"));
    assert_eq!(
        seen[patch + 1..]
            .iter()
            .any(|request| request.method == Method::GET && request.path == after_check),
        succeeded
            || matches!(
                readback,
                Readback::ValidationForbidden | Readback::StaleValidation
            )
    );
    assert_terminal_read(seen, readback, &after_check);
}

fn assert_terminal_read(seen: &[Seen], readback: Readback, after_check: &str) {
    if readback == Readback::ValidationForbidden {
        assert_eq!(
            seen.last().expect("the denied validation read").path,
            after_check
        );
    }
    if readback == Readback::StaleValidation {
        assert_eq!(
            seen.last().expect("the stale validation run").path,
            LINKWARDEN.path(&format!("actions/runs/{CORRECTED_RUN}"))
        );
    }
}

fn assert_patch_reconciliation(seen: &[Seen], patch: usize) {
    for (index, request) in seen
        .iter()
        .enumerate()
        .filter(|(_, request)| request.method == Method::PATCH)
    {
        let body: Value = serde_json::from_str(&request.body).expect("the PATCH body");
        assert_eq!(body, json!({"sha": NEXT_COMMIT, "force": false}));
        assert!(
            seen.get(index + 1).is_some_and(
                |next| next.method == Method::GET && next.path.contains("git/ref/heads/")
            )
        );
        if index > patch {
            assert!(
                seen[patch + 1..index]
                    .iter()
                    .all(|previous| previous.method == Method::GET)
            );
            assert!(
                seen[patch + 1..index]
                    .iter()
                    .any(|previous| previous.path == LINKWARDEN.path("pulls")
                        && previous.query.contains("state=all"))
            );
        }
    }
}
