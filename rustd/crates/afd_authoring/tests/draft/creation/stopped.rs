//! Stop survives Save, activation, and a setup Kill that restores editing.

use super::{
    super::{attempt::Attempt, fixture::now, mint},
    fixture::Case,
};
use afd_authoring::{
    SavedFleet,
    types::{
        DraftPatch, DraftState, MessageQuery, SaveFleet, SaveSource, SendMessage, Source, Start,
    },
};
use afd_core::error_code;
use afd_wire::authoring::{Accounting, Knowledge, Meter, Outcome, Report};
use serde_json::json;

async fn stopped(c: &Case) -> (Attempt, SaveFleet) {
    let attempt = Attempt::start(
        &c.base,
        c.base.access(),
        Source::Definition {
            definition: Box::new(super::super::definition()),
        },
    )
    .await;
    c.base
        .drafts
        .cancel(c.base.access(), &attempt.draft.id, 1)
        .await
        .expect("Stop before Save");
    let draft = c
        .base
        .drafts
        .read(c.base.access(), &attempt.draft.id)
        .await
        .expect("current tag");
    let patch: DraftPatch = serde_json::from_value(
        json!({"runtime_model_ref":c.base.model,"instance_name":"stopped-setup"}),
    )
    .expect("reviewed choices");
    c.base
        .drafts
        .patch(c.base.access(), &draft.id, Some(&draft.etag), patch, now())
        .await
        .expect("idle after Stop");
    let reviewed = c
        .setup
        .read_draft(c.base.access(), &draft.id)
        .await
        .expect("review selected head");
    let request = SaveFleet {
        origin: None,
        name: "stopped-setup".into(),
        start: Start::None,
        source: SaveSource::Draft {
            draft_candidate_id: reviewed.candidate.expect("head").id,
        },
        review_digest: reviewed.review.expect("review").review_digest,
    };
    (attempt, request)
}

async fn late(c: &Case, a: &Attempt) {
    let candidates = c.base.counts().await.1;
    let error = c
        .base
        .drafts
        .activity(&c.base.runner, &a.lease, a.initialize(1))
        .await
        .expect_err("old proposal fenced");
    assert_eq!(error.code(), error_code::AUTHORING_FENCE);
    let request = late_report();
    for replayed in [false, true] {
        let ack = c
            .base
            .drafts
            .report(&c.base.runner, &a.lease, request.clone())
            .await
            .expect("retained accounting");
        assert!(ack.settled);
        assert_eq!(ack.replayed, replayed);
    }
    let page = c
        .base
        .drafts
        .messages(
            c.base.access(),
            &a.draft.id,
            MessageQuery {
                limit: 20,
                starting_after: None,
            },
        )
        .await
        .expect("old transcript survives");
    assert!(
        page.items
            .iter()
            .all(|message| message.text.as_deref() != Some("Old answer must never return"))
    );
    assert_eq!(
        c.base.counts().await.1,
        candidates,
        "no old proposal appended a head"
    );
}

fn late_report() -> Report {
    Report::Terminal {
        answer: Some("Old answer must never return".into()),
        accounting: Accounting {
            fence: "1".into(),
            report_id: mint().to_string(),
            outcome: Outcome::Completed,
            failure_reason: None,
            usage: Meter {
                knowledge: Knowledge::Known,
                calls: 0,
                input_tokens: Some(0),
                output_tokens: Some(0),
                runtime_ms: 1,
                provider_cost_nanos: None,
            },
        },
    }
}

async fn finish_setup(
    c: &Case,
    saved: SavedFleet,
    operation: &afd_core::id::Uuid7,
    request: SaveFleet,
    activate: bool,
) -> DraftState {
    if activate {
        c.fund().await;
        assert_eq!(c.save(operation, request).await.status, "active");
        DraftState::Closed
    } else {
        let killed = c
            .creations
            .patch(
                c.base.access(),
                &saved.id,
                Some(&saved.etag),
                Some("killed"),
                None,
                now(),
            )
            .await
            .expect("setup Kill");
        assert_eq!(killed.status, "killed");
        DraftState::Editing
    }
}

// Value: protects=returning a saved setup to editing never restores old authority;
// fails_when=creation loses the watermark or late reports append a private answer.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn save_after_stop_keeps_old_reports_accounting_only() {
    for activate in [false, true] {
        let c = Case::create().await;
        let (a, request) = stopped(&c).await;
        let operation = mint();
        let saved = c.save(&operation, request.clone()).await;
        assert_eq!(saved.status, "installing");
        let state = finish_setup(&c, saved, &operation, request, activate).await;
        let current = c
            .base
            .drafts
            .read(c.base.access(), &a.draft.id)
            .await
            .expect("draft after setup decision");
        assert_eq!(current.state, state);
        assert_eq!(current.cancelled_through, "1");
        late(&c, &a).await;
        let send = c
            .base
            .drafts
            .send(
                c.base.access(),
                &a.draft.id,
                SendMessage {
                    save: false,
                    message: "New reviewed request".into(),
                    operation_id: mint(),
                },
                now(),
            )
            .await;
        if activate {
            assert_eq!(
                send.expect_err("closed draft cannot restart").code(),
                error_code::AUTHORING_STATE
            );
        } else {
            assert_eq!(
                send.expect("restored editing accepts fresh work").sequence,
                "2"
            );
        }
        c.cleanup().await;
    }
}
