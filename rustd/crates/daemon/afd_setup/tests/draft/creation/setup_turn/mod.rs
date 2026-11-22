//! A draft's setup turn on the run path, through the draft's half of it: the
//! assignment that takes its head message, the live answer it republishes,
//! and the report that finishes it with any save it asked for.
//!
//! The run's own bookkeeping is `afd_fleet`'s; these drive `afd_setup`'s
//! public half directly, on an isolated database, because an assignment walks
//! every draft waiting in its lane.

mod assignment;
mod faults;
mod preview;
mod report;

use super::super::{
    fixture::{Fixture, now},
    mint,
};
use super::fixture::Case;
use af_core::id::Uuid7;
use afd_setup::{
    Access,
    setup_run::{SetupAssignment, SetupFinish, SetupHint, SetupOwed, SetupTurn},
    types::{DraftPatch, SendMessage},
};
use serde_json::json;
use sqlx::Connection as _;

/// What every fixture answer says.
const ANSWER: &str = "Here is the setup.";

/// The fixture runner, platform-owned, so its walk reaches every draft.
async fn seed_runner(f: &Fixture) {
    let policy = afd_db::test_util::RunnerPolicy {
        sandbox_tier: "dev_none",
        network_policy: "deny_all_egress",
    };
    afd_db::test_util::RunnerSeed::new(&f.runner, afd_state::sql::RUNNER_STATUS_ACTIVE, policy)
        .insert(&mut *f.connection().await)
        .await
        .expect("the fixture runner");
}

/// An isolated, funded case with its runner.
async fn funded() -> Case {
    let c = Case::isolated().await;
    c.fund().await;
    seed_runner(&c.base).await;
    c
}

/// A draft of `access`'s holding the message `text`, queued.
async fn queued(f: &Fixture, access: Access<'_>, text: &str) -> Uuid7 {
    f.drafts
        .create(access, f.request(mint(), Some(text)), now())
        .await
        .expect("a draft with its first message")
        .draft
        .id
}

/// Sends `text` to `draft` as its owner; `save` asks to save with it.
async fn send(f: &Fixture, draft: &Uuid7, text: &str, save: bool) {
    let request = SendMessage {
        message: text.to_owned(),
        save,
        operation_id: mint(),
    };
    f.drafts
        .send(f.access(), draft, request, now())
        .await
        .expect("the message is accepted");
}

/// A draft the owner reviewed for Save under `name`, with no message yet.
async fn reviewed(c: &Case, name: &str) -> Uuid7 {
    let draft = c
        .base
        .drafts
        .create(c.base.access(), c.base.request(mint(), None), now())
        .await
        .expect("an idle draft")
        .draft;
    name_draft(c, &draft.id, &draft.etag, name).await;
    draft.id
}

/// Chooses the runtime model and the instance `name` on `draft`.
async fn name_draft(c: &Case, draft: &Uuid7, etag: &str, name: &str) {
    let patch: DraftPatch =
        serde_json::from_value(json!({"runtime_model_ref": c.base.model, "instance_name": name}))
            .expect("runtime and instance choices");
    c.base
        .drafts
        .patch(c.base.access(), draft, Some(etag), patch, now())
        .await
        .expect("select reviewed choices");
}

/// The hint the runner's walk finds for `draft`.
async fn hint(f: &Fixture, draft: &Uuid7) -> SetupHint {
    f.drafts
        .setup_candidates(&f.runner)
        .await
        .expect("the walk reads its candidates")
        .into_iter()
        .find(|hint| &hint.draft_id == draft)
        .expect("the draft waits for a run")
}

/// What one assignment of `hint` found, on a transaction it rolls back.
async fn try_assign(f: &Fixture, hint: &SetupHint) -> afd_setup::Result<SetupAssignment> {
    let mut connection = f.connection().await;
    let mut tx = connection.begin().await.expect("the assignment opens");
    let mut owed = SetupOwed::default();
    let found = f
        .drafts
        .assign_setup_turn(&mut tx, hint, false, &mut owed)
        .await;
    tx.rollback().await.expect("the assignment rolls back");
    found
}

/// The turn an assignment carries, when it took one.
fn assigned(found: SetupAssignment) -> Option<SetupTurn> {
    match found {
        SetupAssignment::Assigned(turn) => Some(*turn),
        SetupAssignment::Passed | SetupAssignment::Busy => None,
    }
}

/// Assigns and starts `draft`'s head message, and commits.
async fn take(f: &Fixture, draft: &Uuid7) -> SetupTurn {
    let hint = hint(f, draft).await;
    let mut connection = f.connection().await;
    let mut tx = connection.begin().await.expect("the assignment opens");
    let mut owed = SetupOwed::default();
    let found = f
        .drafts
        .assign_setup_turn(&mut tx, &hint, false, &mut owed)
        .await
        .expect("the assignment answers");
    let turn = assigned(found).expect("the head message is assigned");
    f.drafts
        .start_setup_turn(&mut tx, &turn)
        .await
        .expect("the message starts");
    tx.commit().await.expect("the assignment commits");
    owed.settle(&f.drafts);
    turn
}

/// Reports `turn`'s answer through `c`'s creations, then runs what the report
/// owes after its commit, as the run plane's report does.
async fn report(
    c: &Case,
    creations: &afd_setup::Creations,
    turn: &SetupTurn,
) -> afd_setup::Result<()> {
    let run = mint();
    let mut tx = sqlx::Transaction::begin(c.base.connection().await, None)
        .await
        .expect("the report opens");
    let scope = creations
        .lock_setup_report(&mut tx, &turn.draft_id, &turn.message_id)
        .await?
        .expect("the draft is still there");
    let mut owed = SetupOwed::default();
    let finish = SetupFinish {
        run_id: &run,
        failure: None,
        answer: Some(ANSWER),
        proposal: None,
        now: now(),
    };
    let announcement = creations
        .finish_setup_turn(&mut tx, &scope, finish, &mut owed)
        .await?;
    creations
        .commit_setup_report(tx, announcement, owed)
        .await?;
    creations.after_setup_report(&scope).await
}

/// The status, answer and refused save's code `turn`'s message holds.
async fn finished(f: &Fixture, turn: &Uuid7) -> (String, Option<String>, Option<String>) {
    sqlx::query_as("SELECT status, answer_text, save_error_code FROM thread.turns WHERE id=$1")
        .bind(turn)
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("the message")
}

/// The fleet a chat save made of `draft`, if any.
async fn saved_fleet(f: &Fixture, draft: &Uuid7) -> Option<(Uuid7, String)> {
    sqlx::query_as(
        "SELECT f.id, f.status FROM setup.drafts d JOIN fleet.fleets f ON f.id = d.fleet_id \
         WHERE d.id=$1",
    )
    .bind(draft)
    .fetch_optional(&mut *f.connection().await)
    .await
    .expect("the draft's fleet")
}
