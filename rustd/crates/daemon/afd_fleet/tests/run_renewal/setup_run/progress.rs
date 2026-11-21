//! The draft stream's `context.progress` keeps a producer once the daemon makes
//! no model call: a setup run's runner reports its compaction on the run's
//! activity verb, and the daemon republishes it on the draft's stream, under
//! the same authority as the run's live answer.

use std::time::Duration;

use af_wire::context::{ContextProgress, ContextState};
use afd_bus::{Received, Subject};
use afd_setup::ContextNotice;

use super::Case;

/// How long a frame may take to arrive, or how long to wait for none.
const WAIT: Duration = Duration::from_secs(5);

// Value: protects=a person whose long setup compacts sees it summarizing, as it did when the daemon
// compacted; fails_when=the daemon drops the run's context_progress frame, publishes it for a
// run the runner no longer holds, or names another draft; why_new=the producer was
// the daemon's own compaction, which is gone; seam=the production draft
// assignment and Drafts::publish_setup_context over an in-process bus.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_setup_runs_compaction_progress_reaches_its_draft_stream() {
    let case = Case::queued().await;
    let (bus, _server) = afd_bus::test_util::detached().expect("an in-process bus opens");
    let drafts = case.drafts.clone().with_bus(bus.clone());
    let taken = case.take_on_host().await;
    let subject = Subject::ThreadItems(case.draft.id.clone());
    let mut stream = bus
        .subscribe(&subject)
        .await
        .expect("the draft stream subscribes");
    let summarizing = ContextProgress {
        generation: 1,
        state: ContextState::Summarizing,
    };

    drafts
        .publish_setup_context(&case.running.runner, &taken.run_id(), &[summarizing])
        .await
        .expect("the live run's progress publishes");
    let notice: ContextNotice = tokio::time::timeout(WAIT, async {
        loop {
            if let Ok(Received::Message(message)) = stream.recv().await
                && let Ok(notice) = serde_json::from_str::<ContextNotice>(&message.payload)
            {
                return notice;
            }
        }
    })
    .await
    .expect("the progress reaches the draft's stream");
    assert_eq!(notice.draft_id, case.draft.id);
    assert_eq!(notice.stream_id, taken.run_id());
    assert_eq!(notice.sequence, "1");
    assert_eq!(notice.progress, summarizing);

    let stranger = afd_db::test_util::mint_id();
    drafts
        .publish_setup_context(&stranger, &taken.run_id(), &[summarizing])
        .await
        .expect("a runner that holds no run is answered, not refused");
    let leaked = tokio::time::timeout(WAIT, async {
        loop {
            match stream.recv().await {
                Ok(Received::Message(message)) => return message,
                Ok(Received::Lagged(_) | Received::Gap) => {}
                Err(_closed) => std::future::pending::<()>().await,
            }
        }
    })
    .await;
    assert!(
        leaked.is_err(),
        "a runner that does not hold the run publishes nothing"
    );
    case.cleanup().await;
}
