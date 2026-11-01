//! CI repair wiring keeps required GitHub reads ahead of bounded writes.

use agentsfleetd::supervisor::Supervisor;
use hyper::Method;
use serde_json::json;

use super::{LINKWARDEN, assert_reconciled_for, assert_repaired_on_named_branch, writes};
use crate::definition_install::install_definition;
use crate::definition_repair::{
    DIAGNOSIS_ONLY, DRAFT_URL, file_at_head, head_reads, routes, script,
};
use crate::definition_responder::{
    ATTEMPT, FAILING_SHA, JOB, JOB_LOG, JOB_LOG_LINE, JOBS, LOG_READER_HOST, RUN,
    ci_evidence_reads, responder_upstream,
};
use crate::definition_run::{assert_token_stayed_on_the_wire, posts, run_event};
use crate::fake_model::FakeModel;
use crate::https::{Seen, Upstream};
use afd_gate::policy::repair::branch_for;
use afd_wire::event::EventType;

#[path = "continuation.rs"]
mod continuation;
#[path = "recovery.rs"]
mod recovery;
#[path = "replay.rs"]
mod replay;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_addressed_repair_reads_evidence_before_draft() {
    let mut supervisor = Supervisor::new();
    let run = install_definition(&mut supervisor, "ci-repairer", &[], None).await;
    let upstream = Upstream::serve(responder_upstream(200)).await;
    let mut reads = ci_evidence_reads();
    reads.extend(head_reads(LINKWARDEN));
    let refused = vec![json!({"url":format!("https://{LOG_READER_HOST}/write"),
        "method":"POST","body":"{}"})];
    let (model, transcript) = FakeModel::deciding(script(LINKWARDEN, reads, refused));
    let settled = run_event(&run, &run.event_id, &upstream, model).await;
    assert!(settled.answer.contains(DRAFT_URL));
    let (seen, asked) = (upstream.seen(), transcript.asked());
    assert_eq!(posts(&seen), writes(LINKWARDEN));
    assert_ci_reads_before_writes(&seen);
    let results = &asked.last().expect("the model completed").results;
    assert!(results.iter().any(|result| result.contains(JOB_LOG_LINE)));
    assert!(
        results
            .iter()
            .any(|result| result.starts_with("[method_not_allowed]"))
    );
    assert_reconciled_for(LINKWARDEN, &branch_for(&run.event_id), &seen);
    assert_repaired_on_named_branch(LINKWARDEN, &run.event_id, &seen, &asked);
    assert_token_stayed_on_the_wire(&seen, &asked);
    assert_existing_draft_is_returned_without_writes(&run).await;
    supervisor.shutdown().await;
    run.cleanup().await;
}

fn assert_ci_reads_before_writes(seen: &[Seen]) {
    let first_write = seen
        .iter()
        .position(|request| request.method == Method::POST)
        .expect("a bounded draft was written");
    for path in [RUN, ATTEMPT, JOBS, JOB, JOB_LOG] {
        assert!(
            seen[..first_write]
                .iter()
                .any(|request| request.path == path),
            "missing {path}"
        );
    }
    assert!(
        seen[..first_write]
            .iter()
            .any(|request| request.query == format!("ref={FAILING_SHA}"))
    );
    assert!(
        seen.iter()
            .filter(|request| request.host == LOG_READER_HOST)
            .all(|request| request.method == Method::GET
                && !request.headers.contains_key("authorization"))
    );
}

async fn assert_existing_draft_is_returned_without_writes(run: &crate::e2e::Scenario) {
    let second = run.enqueue_event(EventType::Chat).await;
    let found = branch_for(&second);
    let upstream = Upstream::serve(routes(LINKWARDEN, Some(&found), file_at_head())).await;
    let (model, transcript) =
        FakeModel::deciding(script(LINKWARDEN, head_reads(LINKWARDEN), Vec::new()));
    let settled = run_event(run, &second, &upstream, model).await;
    assert!(settled.answer.contains(DRAFT_URL));
    let (seen, asked) = (upstream.seen(), transcript.asked());
    assert_reconciled_for(LINKWARDEN, &found, &seen);
    assert_eq!(posts(&seen), Vec::<String>::new());
    assert_eq!(asked.len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_required_ci_log_failure_prevents_all_repair_writes() {
    let mut supervisor = Supervisor::new();
    let run = install_definition(&mut supervisor, "ci-repairer", &[], None).await;
    let upstream = Upstream::serve(responder_upstream(403)).await;
    let mut reads = ci_evidence_reads();
    reads.extend(head_reads(LINKWARDEN));
    let (model, transcript) = FakeModel::deciding(script(LINKWARDEN, reads, Vec::new()));
    let settled = run_event(&run, &run.event_id, &upstream, model).await;
    assert_eq!(settled.answer, DIAGNOSIS_ONLY);
    let seen = upstream.seen();
    assert_eq!(posts(&seen), Vec::<String>::new());
    let asked = transcript.asked();
    assert!(asked.last().expect("model completed").results.iter()
        .any(|result| result.starts_with("[upstream_status]") && result.contains("Status: 403")));
    assert_token_stayed_on_the_wire(&seen, &asked);
    supervisor.shutdown().await;
    run.cleanup().await;
}
