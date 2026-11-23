//! What a capture refuses: plans that cannot hold, custody that lapsed, objects
//! another capture is uploading, manifests altered at rest, and purges this
//! bucket does not own.

use af_core::{error_code::ErrorCode, etag::sha256_hex};
use af_wire::durable_task::{
    CheckpointEntry, CheckpointMode, CheckpointPage, CheckpointPageInput, CheckpointPlan,
    CheckpointPlanInput, DecisionDurableTaskVersion,
};
use afd_fleet::durable_task::{DurableTaskLifecycle as _, DurableTasks};
use sqlx::pool::PoolConnection;

use super::{
    super::fixture::{Case, id},
    checkpoint::{input, valid},
};

mod custody;
mod manifest;
mod objects;
mod plan;
mod purge;

/// The case database, for the reads and alterations a scenario makes beside the verbs.
async fn connection(case: &Case) -> PoolConnection<sqlx::Postgres> {
    case.held
        .fixtures
        .database
        .acquire()
        .await
        .expect("scenario observer")
}

/// The task's target becomes an API-only objective: no repository, so no commit.
async fn api_only(case: &Case, task: &DecisionDurableTaskVersion) {
    let changed = sqlx::query(
        "UPDATE fleet.durable_tasks SET target = '{\"kind\":\"api_only\"}'::jsonb WHERE id = $1",
    )
    .bind(id(&task.durable_task_id))
    .execute(&mut *connection(case).await)
    .await
    .expect("the task becomes API-only");
    assert_eq!(changed.rows_affected(), 1);
}

/// A fresh incremental plan request on an API-only task: no base, no commit.
fn incremental(case: &Case, task: &DecisionDurableTaskVersion, pages: u32) -> CheckpointPlanInput {
    let mut request = input(task, case, CheckpointMode::Incremental, None, "");
    request.commit_oid = None;
    request.page_count = pages;
    request
}

/// Plans `request`, which must be admitted.
async fn planned(
    tasks: &DurableTasks,
    case: &Case,
    request: CheckpointPlanInput,
) -> CheckpointPlan {
    tasks
        .plan_checkpoint(case.run(), &valid(request))
        .await
        .expect("an admissible plan is allocated")
}

/// Ends `plan`'s transfer window, so the task's next plan abandons it: a task holds
/// one unfinished capture at a time.
async fn lapse(case: &Case, plan: &CheckpointPlan) {
    let lapsed = sqlx::query(
        "UPDATE runner.checkpoints SET expires_at = now() - interval '1 minute' WHERE id = $1",
    )
    .bind(id(&plan.checkpoint_id))
    .execute(&mut *connection(case).await)
    .await
    .expect("the capture window ends");
    assert_eq!(lapsed.rows_affected(), 1);
}

/// One manifest page of `entries` and `history`, at `number`.
fn page(number: u32, entries: Vec<CheckpointEntry>, history: Vec<String>) -> CheckpointPage {
    CheckpointPage {
        page_number: number,
        entries,
        history_objects: history,
    }
}

/// A directory entry at `path`.
fn directory(path: &str) -> CheckpointEntry {
    CheckpointEntry::Directory { path: path.into() }
}

/// The digest a page's canonical bytes carry.
fn digest_of(contents: &CheckpointPage) -> String {
    sha256_hex(&serde_json::to_vec(contents).expect("page JSON"))
}

/// Writes `contents` as `plan`'s page at its own number, which must be admitted.
async fn write(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &CheckpointPlan,
    contents: CheckpointPage,
) -> (String, String) {
    let position = usize::try_from(contents.page_number).expect("small page number");
    let page = plan.page_ids.get(position).expect("allocated page").clone();
    let digest = digest_of(&contents);
    tasks
        .write_checkpoint_page(
            case.run(),
            &id(&plan.checkpoint_id),
            &id(&page),
            &valid(CheckpointPageInput {
                fencing_token: case.run().fencing_token,
                durable_task: task.clone(),
                pin_id: plan.pin.id.clone(),
                digest: digest.clone(),
                contents,
            }),
        )
        .await
        .expect("a valid page is frozen");
    (page, digest)
}

/// Asserts `result` failed with `code`, naming `why`.
fn refused<T: std::fmt::Debug>(result: Result<T, afd_fleet::Error>, code: ErrorCode, why: &str) {
    let failure = result.expect_err(why);
    assert_eq!(failure.code(), code, "{why}: {failure}");
}

/// The two page tables a manifest is frozen in.
#[derive(Clone, Copy)]
enum Pages {
    Overlay,
    Base,
}

impl Pages {
    const fn select(self) -> &'static str {
        match self {
            Self::Overlay => {
                "SELECT id, checkpoint_id AS owner, page_number, digest, body FROM runner.checkpoint_pages WHERE id = $1"
            }
            Self::Base => {
                "SELECT id, checkpoint_base_id AS owner, page_number, digest, body FROM runner.checkpoint_pages WHERE id = $1"
            }
        }
    }

    const fn insert(self) -> &'static str {
        match self {
            Self::Overlay => {
                "INSERT INTO runner.checkpoint_pages (id, checkpoint_id, page_number, digest, body, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, now(), now())"
            }
            Self::Base => {
                "INSERT INTO runner.checkpoint_pages (id, checkpoint_base_id, page_number, digest, body, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, now(), now())"
            }
        }
    }
}

/// One frozen page row as stored. A frozen row admits no `UPDATE`, so an
/// alteration at rest is the delete-and-insert the runtime role may still make.
#[derive(Clone, sqlx::FromRow)]
struct Stored {
    id: af_core::id::Uuid7,
    owner: af_core::id::Uuid7,
    page_number: i32,
    digest: String,
    body: Vec<u8>,
}

impl Stored {
    async fn read(case: &Case, pages: Pages, page: &str) -> Self {
        sqlx::query_as(pages.select())
            .bind(id(page))
            .fetch_one(&mut *connection(case).await)
            .await
            .expect("the frozen page row")
    }

    /// Overlay and base pages share one table, so a page id alone deletes it.
    async fn delete(case: &Case, page: &af_core::id::Uuid7) {
        let gone = sqlx::query("DELETE FROM runner.checkpoint_pages WHERE id = $1")
            .bind(page)
            .execute(&mut *connection(case).await)
            .await
            .expect("the page row is deleted");
        assert_eq!(gone.rows_affected(), 1);
    }

    /// This row in place of the one stored under its id.
    async fn replace(&self, case: &Case, pages: Pages) {
        Self::delete(case, &self.id).await;
        self.replace_missing(case, pages).await;
    }

    /// This row stored again where no row holds its id.
    async fn replace_missing(&self, case: &Case, pages: Pages) {
        sqlx::query(pages.insert())
            .bind(&self.id)
            .bind(&self.owner)
            .bind(self.page_number)
            .bind(&self.digest)
            .bind(&self.body)
            .execute(&mut *connection(case).await)
            .await
            .expect("the page row is stored again");
    }
}
