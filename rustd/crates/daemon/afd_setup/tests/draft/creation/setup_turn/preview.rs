//! A setup run's live answer and compaction progress reach the draft's private
//! stream only while the run is its runner's live one and its creator still
//! holds the draft; a chunk or status no frame can carry is dropped. Deleting
//! the draft tells that run's runner to stop.

use std::time::Duration;

use super::{Case, Fixture, funded, mint, queued, take};
use af_core::{error_code, id::Uuid7};
use af_wire::context::{ContextProgress, ContextState};
use afd_bus::{Command, Received, Subject, Subscription};
use afd_setup::{ContextNotice, Delta, Drafts, setup_run::PreviewChunk};
use sqlx::Connection as _;

/// How long a frame may take to arrive, or how long to wait for none.
const WAIT: Duration = Duration::from_millis(500);

/// A live run of draft `$3`'s message `$6` on runner `$2`, at the thread's fence.
const RUN: &str = "INSERT INTO thread.runs \
    (id, runner_id, draft_id, workspace_id, tenant_id, turn_id, turn_created_at, \
     posture, provider, model, last_metered_at, fencing_token, run_expires_at, \
     status, created_at, updated_at) \
    SELECT $1,$2,$3,$4,$5,$6,now(),$7,'fixture-provider','fixture-model', \
           now(),t.fencing_token,now() + interval '1 hour',$8,now(),now() \
      FROM thread.threads t WHERE t.id = $3";

/// The draft's thread, held by its run for the next hour.
const HOLD: &str =
    "UPDATE thread.threads SET run_expires_at = now() + interval '1 hour' WHERE id=$1";

/// The largest answer one frame carries, in bytes: the draft frame less its
/// envelope, at JSON's worst escape.
const FRAME_TEXT_BYTES: usize = (afd_bus::DRAFT_FRAME_BYTES - 4096) / 6;

/// Seeds a live run of `draft`'s running message `message`, and answers it.
async fn live_run(f: &Fixture, draft: &Uuid7, message: &Uuid7) -> Uuid7 {
    let run = mint();
    let mut connection = f.connection().await;
    sqlx::query(RUN)
        .bind(&run)
        .bind(&f.runner)
        .bind(draft)
        .bind(&f.workspace)
        .bind(&f.tenant)
        .bind(message)
        .bind(afd_billing::Posture::SelfManaged.as_str())
        .bind(afd_state::sql::RUN_STATUS_ACTIVE)
        .execute(&mut *connection)
        .await
        .expect("the draft's live run");
    sqlx::query(HOLD)
        .bind(draft)
        .execute(&mut *connection)
        .await
        .expect("the run holds its thread");
    run
}

/// Every payload `stream` receives within [`WAIT`].
async fn drain(stream: &mut Subscription) -> Vec<String> {
    let mut payloads = Vec::new();
    while let Ok(Ok(received)) = tokio::time::timeout(WAIT, stream.recv()).await {
        if let Received::Message(message) = received {
            payloads.push(message.payload.to_string());
        }
    }
    payloads
}

/// A case with a running message, its live run, and `drafts` over an
/// in-process bus with the draft's stream subscribed.
struct Live {
    c: Case,
    drafts: Drafts,
    draft: Uuid7,
    run: Uuid7,
    stream: Subscription,
    drafts_bus: afd_bus::Handle,
    _server: afd_bus::test_util::Server,
}

async fn live(access_member: bool) -> Live {
    let c = funded().await;
    let access = if access_member {
        c.base.member_access()
    } else {
        c.base.access()
    };
    let draft = queued(&c.base, access, "Watch checkout").await;
    let turn = take(&c.base, &draft).await;
    let run = live_run(&c.base, &draft, &turn.message_id).await;
    let (bus, server) = afd_bus::test_util::detached().expect("an in-process bus opens");
    let stream = bus
        .subscribe(&Subject::ThreadItems(draft.clone()))
        .await
        .expect("the draft stream subscribes");
    Live {
        drafts: c.base.drafts.clone().with_bus(bus.clone()),
        drafts_bus: bus,
        c,
        draft,
        run,
        stream,
        _server: server,
    }
}

// Value: protects=a live answer reaches the draft's stream in order, with a
// chunk no frame carries and a status out of bounds dropped; fails_when=the
// size or bound check is dropped, or a frame names another stream;
// seam=Drafts::publish_setup_preview and publish_setup_context over an
// in-process bus and live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_live_answer_reaches_the_draft_stream_without_what_no_frame_carries() {
    let mut live = live(false).await;
    let oversized = "x".repeat(FRAME_TEXT_BYTES + 1);
    let chunks = [
        PreviewChunk {
            stream_seq: 0,
            stream_start: true,
            text: "Here ",
        },
        PreviewChunk {
            stream_seq: 1,
            stream_start: false,
            text: "",
        },
        PreviewChunk {
            stream_seq: 2,
            stream_start: false,
            text: &oversized,
        },
        PreviewChunk {
            stream_seq: 3,
            stream_start: false,
            text: "it is.",
        },
    ];
    live.drafts
        .publish_setup_preview(&live.c.base.runner, &live.run, &chunks)
        .await
        .expect("the live run's answer publishes");
    let deltas: Vec<(u64, bool, String)> = drain(&mut live.stream)
        .await
        .iter()
        .map(|payload| {
            let delta: Delta = serde_json::from_str(payload).expect("a delta frame");
            assert_eq!(
                (&delta.draft_id, &delta.stream_id),
                (&live.draft, &live.run)
            );
            assert_eq!(delta.sequence, "1");
            (delta.stream_seq, delta.stream_start, delta.text.clone())
        })
        .collect();
    assert_eq!(
        deltas,
        [
            (1, true, "Here ".to_owned()),
            (4, false, "it is.".to_owned())
        ]
    );

    let unbounded = ContextProgress {
        generation: 0,
        state: ContextState::Summarizing,
    };
    let resuming = ContextProgress {
        generation: 1,
        state: ContextState::Resuming,
    };
    live.drafts
        .publish_setup_context(&live.c.base.runner, &live.run, &[unbounded, resuming])
        .await
        .expect("the live run's progress publishes");
    let notices: Vec<ContextProgress> = drain(&mut live.stream)
        .await
        .iter()
        .map(|payload| {
            serde_json::from_str::<ContextNotice>(payload)
                .expect("a progress frame")
                .progress
        })
        .collect();
    assert_eq!(notices, [resuming]);
    live.c.cleanup().await;
}

// Value: protects=private text never reaches a stream with no live run behind
// it, nor one whose creator lost the workspace; fails_when=the authority read
// is skipped for a stranger runner or a revoked creator; seam=
// Drafts::publish_setup_preview over an in-process bus and live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_preview_without_its_authority_publishes_nothing() {
    let mut live = live(true).await;
    let chunk = [PreviewChunk {
        stream_seq: 0,
        stream_start: true,
        text: "Private",
    }];
    let progress = [ContextProgress {
        generation: 1,
        state: ContextState::Summarizing,
    }];
    let busless = live.c.base.drafts.clone();
    busless
        .publish_setup_preview(&live.c.base.runner, &live.run, &chunk)
        .await
        .expect("no bus publishes nothing");
    busless
        .publish_setup_context(&live.c.base.runner, &live.run, &progress)
        .await
        .expect("no bus publishes nothing");
    live.drafts
        .publish_setup_preview(&mint(), &live.run, &chunk)
        .await
        .expect("a stranger runner is answered, not refused");
    sqlx::query("DELETE FROM identity.memberships WHERE tenant_id=$1 AND user_id=$2")
        .bind(&live.c.base.tenant)
        .bind(&live.c.base.member)
        .execute(&mut *live.c.base.connection().await)
        .await
        .expect("revoke the member");
    live.drafts
        .publish_setup_preview(&live.c.base.runner, &live.run, &chunk)
        .await
        .expect("a revoked creator is answered, not refused");
    assert_eq!(drain(&mut live.stream).await, Vec::<String>::new());
    live.c.cleanup().await;
}

// Value: protects=a bus that refuses a frame, and a store that stops answering
// the authority read, are reported to the run plane rather than swallowed;
// fails_when=either failure is dropped as a cosmetic miss; seam=
// Drafts::publish_setup_preview and publish_setup_context over a refusing bus
// and a private live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_refused_frame_and_an_unanswered_authority_read_are_reported() {
    let live = live(false).await;
    let refusing = live
        .c
        .base
        .drafts
        .clone()
        .with_bus(afd_bus::test_util::unreachable());
    let chunk = [PreviewChunk {
        stream_seq: 0,
        stream_start: true,
        text: "Here",
    }];
    let progress = [ContextProgress {
        generation: 1,
        state: ContextState::Summarizing,
    }];
    let runner = &live.c.base.runner;
    let refused = refusing
        .publish_setup_preview(runner, &live.run, &chunk)
        .await
        .expect_err("the bus refuses the answer");
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
    let refused = refusing
        .publish_setup_context(runner, &live.run, &progress)
        .await
        .expect_err("the bus refuses the progress");
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);

    let mut owner = sqlx::PgConnection::connect(&live.c.base.owner_url())
        .await
        .expect("the lane owner connects");
    let mut held = sqlx::Connection::begin(&mut owner)
        .await
        .expect("the holder opens");
    sqlx::query("LOCK TABLE identity.workspaces IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *held)
        .await
        .expect("the owner takes the table");
    let unanswered = live
        .drafts
        .publish_setup_preview(runner, &live.run, &chunk)
        .await
        .expect_err("the creator's read never answers");
    assert_eq!(unanswered.code(), error_code::INTERNAL_DB_QUERY);
    held.rollback().await.expect("the owner releases the table");
    drop(owner);
    live.c.cleanup().await;
}

// Value: protects=deleting a draft tells its live run's runner to stop, once
// the delete committed; fails_when=the purge's killed runs are not announced;
// seam=Drafts::delete over an in-process bus and live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn deleting_a_draft_tells_its_live_runs_runner_to_cancel() {
    let live = live(false).await;
    let mut commands = live
        .drafts_bus
        .subscribe(&Subject::RunnerCommand(live.c.base.runner.clone()))
        .await
        .expect("the runner's commands subscribe");
    live.drafts
        .delete(live.c.base.access(), &live.draft)
        .await
        .expect("the creator deletes the draft");
    let mut received = Vec::new();
    while let Ok(Ok(Received::Message(message))) = tokio::time::timeout(WAIT, commands.recv()).await
    {
        received.push(serde_json::from_str::<Command>(&message.payload).expect("a command"));
    }
    assert_eq!(
        received,
        [Command::Cancel {
            run_id: live.run.clone()
        }]
    );
    live.c.cleanup().await;
}
