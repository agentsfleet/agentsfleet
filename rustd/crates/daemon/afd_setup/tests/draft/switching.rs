//! Model switches and Stop against queued work.
//!
//! A switch is an idle, If-Match edit: it refuses a model the workspace owner
//! has taken away and refuses while a request is still queued, leaving the
//! draft exactly as it was. Stop and send both take the draft row, so their
//! race is forced in each order by holding that row and releasing it once both
//! requests are observed queued behind it — no sleeps.

use super::{
    contention::waiting,
    fixture::{Fixture, now},
    mint,
};
use af_core::error_code::{SETUP_MODEL, SETUP_STATE};
use af_core::id::Uuid7;
use afd_setup::types::{Draft, DraftPatch, MessageQuery, MessageState, SendMessage};
use afd_turns::view::stop::Stop;
use serde_json::json;
use sqlx::Connection as _;
use std::time::Duration;

const LIMIT: Duration = Duration::from_secs(10);

/// Ways the workspace owner takes a model away from a draft.
const WITHDRAWALS: [(&str, &str); 2] = [
    (
        "entry removed",
        "DELETE FROM model.tenant_models WHERE id=$1",
    ),
    (
        "credential removed",
        "DELETE FROM vault.secrets WHERE key_name='setup-model' \
         AND workspace_id=(SELECT w.id FROM identity.workspaces w \
         JOIN model.tenant_models e ON e.tenant_id=w.tenant_id WHERE e.id=$1)",
    ),
];

fn switch_to(model: &Uuid7) -> DraftPatch {
    serde_json::from_value(json!({"setup_model_ref": model})).expect("typed patch")
}

fn send(text: &str) -> SendMessage {
    SendMessage {
        save: false,
        message: text.into(),
        operation_id: mint(),
    }
}

async fn draft(f: &Fixture, message: Option<&str>) -> Draft {
    f.drafts
        .create(f.access(), f.request(mint(), message), now())
        .await
        .expect("native draft")
        .draft
}

/// Every request's state, in sequence order.
async fn states(f: &Fixture, id: &Uuid7) -> Vec<(String, MessageState)> {
    let query = MessageQuery {
        limit: 25,
        starting_after: None,
    };
    let page = f
        .drafts
        .messages(f.access(), id, query)
        .await
        .expect("page");
    page.items
        .into_iter()
        .filter(|message| message.role == "user")
        .map(|message| (message.sequence, message.state))
        .collect()
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_switch_to_a_withdrawn_model_is_refused_and_changes_nothing() {
    for (how, withdraw) in WITHDRAWALS {
        let f = Fixture::create().await;
        let created = draft(&f, None).await;
        sqlx::query(withdraw)
            .bind(&f.other_model)
            .execute(&mut *f.connection().await)
            .await
            .expect("withdraw the model");
        let refused = f
            .drafts
            .patch(
                f.access(),
                &created.id,
                Some(&created.etag),
                switch_to(&f.other_model),
                now(),
            )
            .await
            .expect_err("a withdrawn model cannot be selected");
        assert_eq!(refused.code(), SETUP_MODEL, "{how}");
        let after = f.drafts.read(f.access(), &created.id).await.expect("read");
        assert_eq!(after.etag, created.etag, "{how}: nothing was written");
        assert_eq!(after.setup_model_ref, f.model.to_string(), "{how}");
        assert_eq!(
            serde_json::to_value(&after.candidate).expect("candidate JSON"),
            serde_json::to_value(&created.candidate).expect("candidate JSON"),
            "{how}: the reviewed candidate survives"
        );
        f.cleanup().await;
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_switch_waits_until_no_request_is_queued() {
    let f = Fixture::create().await;
    let created = draft(&f, Some("Investigate checkout")).await;
    let refused = f
        .drafts
        .patch(
            f.access(),
            &created.id,
            Some(&created.etag),
            switch_to(&f.other_model),
            now(),
        )
        .await
        .expect_err("a queued request pins the current model");
    assert_eq!(refused.code(), SETUP_STATE);
    assert_eq!(refused.current_state(), Some("setup"), "{refused}");
    let queued = f.drafts.read(f.access(), &created.id).await.expect("read");
    assert_eq!(queued.etag, created.etag, "the refusal wrote nothing");
    assert_eq!(queued.setup_model_ref, f.model.to_string());

    f.stop(&created.id, 1)
        .await
        .expect("stop the queued request");
    let idle = f.drafts.read(f.access(), &created.id).await.expect("read");
    let switched = f
        .drafts
        .patch(
            f.access(),
            &created.id,
            Some(&idle.etag),
            switch_to(&f.other_model),
            now(),
        )
        .await
        .expect("an idle draft switches");
    assert_eq!(switched.setup_model_ref, f.other_model.to_string());
    f.cleanup().await;
}

/// Stop names one turn; a send racing it lands in either order and stays
/// queued.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn send_and_stop_race_in_either_order_without_stopping_the_later_request() {
    let f = Fixture::create().await;
    for stop_first in [true, false] {
        let id = draft(&f, Some("first")).await.id;
        // Boxed: each operation's future is tens of kilobytes, and the pair
        // would otherwise sit inline in this test's own future.
        let stop = Box::pin(f.stop(&id, 1));
        let later = Box::pin(f.drafts.send(f.access(), &id, send("second"), now()));
        let (stopped, sent) = if stop_first {
            in_order(&f, &id, stop, later).await
        } else {
            let (sent, stopped) = in_order(&f, &id, later, stop).await;
            (stopped, sent)
        };
        let stopped = stopped
            .expect("Stop the first request")
            .expect("the draft holds turn 1");
        assert_eq!(stopped.sequence, 1, "stop_first={stop_first}");
        assert!(
            matches!(stopped.stop, Stop::Cancelled { turns: 1, .. }),
            "stop_first={stop_first}"
        );
        assert_eq!(sent.expect("send").sequence, "2", "stop_first={stop_first}");
        assert_eq!(
            states(&f, &id).await,
            [
                ("1".to_owned(), MessageState::Cancelled),
                ("2".to_owned(), MessageState::Queued)
            ],
            "stop_first={stop_first}"
        );
    }
    f.cleanup().await;
}

/// Runs `first` then `second` against the draft row in that order: the row is
/// held, `first` is started and observed queued behind it, then `second`, and
/// only then is the row released. Postgres grants a held row to its waiters in
/// the order they queued.
async fn in_order<A: Future, B: Future>(
    f: &Fixture,
    draft: &Uuid7,
    first: A,
    second: B,
) -> (A::Output, B::Output) {
    let mut connection = f.connection().await;
    let mut lock = connection.begin().await.expect("hold the draft");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *lock)
        .await
        .expect("lock owner");
    sqlx::query("SELECT id FROM setup.drafts WHERE id=$1 FOR UPDATE")
        .bind(draft)
        .fetch_one(&mut *lock)
        .await
        .expect("hold the private row");
    let later = async {
        waiting(f, pid, 1).await;
        let release = async {
            waiting(f, pid, 2).await;
            lock.commit().await.expect("release both in queued order");
        };
        let (outcome, ()) = tokio::join!(second, release);
        outcome
    };
    tokio::time::timeout(LIMIT, async { tokio::join!(first, later) })
        .await
        .expect("both requests finish once the row is released")
}
