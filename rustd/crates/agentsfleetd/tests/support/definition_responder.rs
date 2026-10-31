//! Scripted GitHub CI evidence for native fleet wiring tests.

use afr_providers::Chunk;
use hyper::Method;
use serde_json::{Value, json};

use crate::definition_repair::{FILE, GITHUB, Repo, file_at_head, github_auth, head_reads, routes};
use crate::fake_model::{call, http, say};
use crate::https::{Reply, Route};

/// The read-only and repair fixtures investigate the same repository and base.
const REPOSITORY: Repo = Repo {
    name: "agentsfleet/linkwarden",
    base: "dev",
};
/// The selected run, attempt and failed job.
pub(crate) const RUN: &str = "/repos/agentsfleet/linkwarden/actions/runs/101";
pub(crate) const ATTEMPT: &str = "/repos/agentsfleet/linkwarden/actions/runs/101/attempts/1";
pub(crate) const JOBS: &str = "/repos/agentsfleet/linkwarden/actions/runs/101/attempts/1/jobs";
pub(crate) const JOB: &str = "/repos/agentsfleet/linkwarden/actions/jobs/202";
pub(crate) const JOB_LOG: &str = "/repos/agentsfleet/linkwarden/actions/jobs/202/logs";
pub(crate) const FAILING_SHA: &str = "c0ffee1000000000000000000000000000000000";
pub(crate) const RUN_URL: &str = "https://github.com/agentsfleet/linkwarden/actions/runs/101";
pub(crate) const FAILED_STEP: &str = "Run unit tests";
/// The exact reviewed download host, never a wildcard or automatic enrollment.
pub(crate) const LOG_READER_HOST: &str = "productionresultssa6.blob.core.windows.net";
pub(crate) const JOB_LOG_LINE: &str = "2026-10-30T08:35:25Z assertion failed: retries must be 3";
pub(crate) const LOG_SIGNATURE: &str = "sig=fixture-signature";
/// The answer and memory key belong to the selected attempt.
pub(crate) const DIAGNOSIS: &str = "Run https://github.com/agentsfleet/linkwarden/actions/runs/101 \
    attempt 1 failed at commit c0ffee1000000000000000000000000000000000. \
    Job 202, step Run unit tests ran 2026-10-30T08:35:24Z to 2026-10-30T08:35:26Z. \
    Its returned log says: 2026-10-30T08:35:25Z assertion failed: retries must be 3. \
    src/lib/fetch.ts at that commit sets retries to 0; the separately read dev \
    head has the same code. A small forward fix sets retries to 3. No repository write was made.";
pub(crate) const SOURCE_GAP: &str = "Evidence gap: run 101 attempt 1, GET job 202 logs \
    returned HTTP 403. Run, attempt and failed-job metadata were read; required \
    job-log text is unavailable. Restore download access before investigating. No fix is proposed.";
pub(crate) const MEMORY_KEY: &str = "ci:linkwarden:101:1";

fn run_metadata() -> Value {
    json!({"id":101,"run_attempt":1,"head_sha":FAILING_SHA,"html_url":RUN_URL,
        "conclusion":"failure","repository":{"full_name":"agentsfleet/linkwarden"}})
}

fn job_metadata() -> Value {
    json!({"id":202,"run_id":101,"run_attempt":1,"head_sha":FAILING_SHA,
        "name":"test","conclusion":"failure",
        "started_at":"2026-10-30T08:33:29Z","completed_at":"2026-10-30T08:35:32Z",
        "steps":[{"name":FAILED_STEP,"number":8,"conclusion":"failure",
            "started_at":"2026-10-30T08:35:24Z","completed_at":"2026-10-30T08:35:26Z"}]})
}

/// One authenticated GitHub read; no body can turn it into a write.
fn github_read(path: &str) -> Value {
    json!({"url":format!("https://{GITHUB}{path}"),"headers":github_auth()})
}

/// A GET to a GitHub path with the brokered credential.
pub(crate) fn github_get(id: &str, path: &str) -> Chunk {
    http(id, github_read(path))
}

/// Required CI reads, before current-base reads or any repair write.
pub(crate) fn ci_evidence_reads() -> Vec<Value> {
    let mut reads = vec![
        github_read(RUN),
        github_read(ATTEMPT),
        github_read(&format!("{JOBS}?per_page=100&page=1")),
        github_read(JOB),
        github_read(JOB_LOG),
    ];
    reads.push(github_read(
        &REPOSITORY.path(&format!("contents/{FILE}?ref={FAILING_SHA}")),
    ));
    reads
}

/// A separate fleet reads the job metadata and text through real guarded TLS.
pub(crate) fn log_reader_routes(redirect_status: u16, status: u16) -> Vec<Route> {
    let mut result = vec![
        Route::new(
            GITHUB,
            Method::GET,
            RUN,
            vec![Reply::json(200, &run_metadata())],
        ),
        Route::new(
            GITHUB,
            Method::GET,
            JOB,
            vec![Reply::json(200, &job_metadata())],
        ),
    ];
    result.extend(log_routes(redirect_status, status));
    result
}

fn log_routes(redirect_status: u16, status: u16) -> Vec<Route> {
    vec![
        Route::new(
            GITHUB,
            Method::GET,
            JOB_LOG,
            vec![Reply::redirect(redirect_status, "log-content")],
        ),
        Route::new(
            GITHUB,
            Method::GET,
            "/repos/agentsfleet/linkwarden/actions/jobs/202/log-content",
            vec![Reply::redirect(
                redirect_status,
                &format!("https://{LOG_READER_HOST}/download/job?{LOG_SIGNATURE}"),
            )],
        ),
        Route::new(
            LOG_READER_HOST,
            Method::GET,
            "/download/job",
            vec![Reply::text(
                status,
                &format!(
                    "{JOB_LOG_LINE}\necho=https://{LOG_READER_HOST}/download/job?{LOG_SIGNATURE}\njob=202"
                ),
            )],
        ),
    ]
}

/// GitHub serves both the failed revision and current base; their file bytes match.
pub(crate) fn responder_upstream(log_status: u16) -> Vec<Route> {
    let mut result = routes(REPOSITORY, None, file_at_head());
    result.extend(log_reader_routes(302, log_status));
    result.extend([
        Route::new(
            GITHUB,
            Method::GET,
            ATTEMPT,
            vec![Reply::json(200, &run_metadata())],
        ),
        Route::new(
            GITHUB,
            Method::GET,
            JOBS,
            vec![Reply::json(
                200,
                &json!({"total_count":1,"jobs":[job_metadata()]}),
            )],
        ),
    ]);
    result
}

/// The scripted model reads required evidence, then the current trusted base.
pub(crate) fn responder_turns() -> Vec<Vec<Chunk>> {
    let mut result: Vec<_> = ci_evidence_reads()
        .into_iter()
        .enumerate()
        .map(|(index, read)| vec![http(&format!("evidence-{index}"), read)])
        .collect();
    result.push(
        head_reads(REPOSITORY)
            .into_iter()
            .enumerate()
            .map(|(index, read)| http(&format!("head-{index}"), read))
            .collect(),
    );
    result.push(vec![call(
        "recall",
        "memory_recall",
        json!({"query":"linkwarden"}),
    )]);
    result.push(vec![call(
        "store",
        "memory_store",
        json!({"key":MEMORY_KEY,"content":DIAGNOSIS,"category":"core"}),
    )]);
    result.push(vec![say(DIAGNOSIS)]);
    result
}

/// A failed log read ends the scripted model before source or memory calls.
pub(crate) fn responder_stopping_turns() -> Vec<Vec<Chunk>> {
    let mut result: Vec<_> = ci_evidence_reads()
        .into_iter()
        .take(5)
        .enumerate()
        .map(|(index, read)| vec![http(&format!("evidence-{index}"), read)])
        .collect();
    result.push(vec![say(SOURCE_GAP)]);
    result
}
