//! Distinct GitHub check-run, Actions run, attempt and job identities.
use super::{NEXT_COMMIT, Readback, Remote, State};
use crate::definition_repair::{GITHUB, NEW_COMMIT};
use crate::https::{Reply, Seen};
use serde_json::{Value, json};

pub(crate) const CHECK_RUN: u64 = 42;
pub(crate) const VALIDATION_RUN: u64 = 177;
pub(crate) const VALIDATION_JOB: u64 = 430;
pub(crate) const CORRECTED_CHECK: u64 = 43;
pub(crate) const CORRECTED_RUN: u64 = 178;
pub(crate) const CORRECTED_JOB: u64 = 431;

impl Remote {
    pub(super) fn validation(&self, state: &State, request: &Seen) -> Reply {
        for (head, run, job, check, conclusion) in [
            (
                NEW_COMMIT,
                VALIDATION_RUN,
                VALIDATION_JOB,
                CHECK_RUN,
                "failure",
            ),
            (
                NEXT_COMMIT,
                CORRECTED_RUN,
                CORRECTED_JOB,
                CORRECTED_CHECK,
                "success",
            ),
        ] {
            if request.path == self.repo.path(&format!("actions/runs/{run}")) {
                return Reply::json(
                    200,
                    &json!({"id": run, "run_attempt": 1, "head_sha": if run == CORRECTED_RUN && self.readback == Readback::StaleValidation {NEW_COMMIT} else {head}}),
                );
            }
            let value = self.validation_job(head, run, job, check, conclusion);
            if request.path
                == self
                    .repo
                    .path(&format!("actions/runs/{run}/attempts/1/jobs"))
            {
                return Reply::json(200, &json!({"total_count": 1, "jobs": [value]}));
            }
            if request.path == self.repo.path(&format!("actions/jobs/{job}")) {
                return Reply::json(200, &value);
            }
        }
        if request.path
            == self
                .repo
                .path(&format!("actions/jobs/{VALIDATION_JOB}/logs"))
        {
            return Reply::text(
                200,
                "Regression failed at the initial draft head: expected retry count 4, received 3.\n",
            );
        }
        self.validation_checks(state, request)
    }

    fn validation_checks(&self, state: &State, request: &Seen) -> Reply {
        let head = if request.path.contains(NEXT_COMMIT) {
            NEXT_COMMIT
        } else {
            NEW_COMMIT
        };
        if head == NEXT_COMMIT && self.readback == Readback::ValidationForbidden {
            return Reply::json(403, &json!({"message": "Validation read denied"}));
        }
        let conclusion = if head == NEXT_COMMIT && state.reference.as_deref() == Some(NEXT_COMMIT) {
            "success"
        } else {
            "failure"
        };
        Reply::json(
            200,
            &json!({"total_count": 1, "check_runs": [{"id": if head == NEXT_COMMIT {CORRECTED_CHECK} else {CHECK_RUN}, "name": "title-regression", "head_sha": head,
            "status": "completed", "conclusion": conclusion, "details_url": format!("https://github.com/{}/actions/runs/{}", self.repo.name, if head == NEXT_COMMIT {CORRECTED_RUN} else {VALIDATION_RUN})}]}),
        )
    }

    fn validation_job(
        &self,
        head: &str,
        run: u64,
        job: u64,
        check: u64,
        conclusion: &str,
    ) -> Value {
        json!({"id": job, "run_id": run, "run_attempt": 1, "head_sha": head,
            "conclusion": conclusion, "check_run_url": format!("https://{GITHUB}/repos/{}/check-runs/{check}", self.repo.name),
            "steps": [{"name": "Check retry count", "conclusion": conclusion}]})
    }
}
