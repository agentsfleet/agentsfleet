//! Unsettled acknowledgements retain one worker and report through private-answer expiry.

use super::{json, lease, result, rig};
use crate::{client::Verb, test_support::Rig};
use af_core::timing::{DAY, STREAM_ANSWER_TIMEOUT};
use serde_json::Value;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{sync::mpsc, time::Instant};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

type Sent = (Instant, Value);

// Value: protects=private answer expiry preserves one accounting identity until settled;
// fails_when=retries extend private retention, lose usage, finish on unsettled, or redispatch;
// why_new=worker fixtures otherwise settle immediately and daemon tests build accounting by hand;
// seam=the existing session correlator and scripted daemon; no_claim=carrier loss or live billing.
#[tokio::test(start_paused = true)]
async fn an_unsettled_report_expires_its_answer_and_keeps_accounting_until_settled() {
    let mut delivery = Delivery::start();
    let (first_at, terminal) = delivery.next_report().await;
    assert_eq!(terminal["kind"], "terminal");
    assert_eq!(terminal["answer"], "Private result");
    assert_eq!(terminal["usage"]["calls"], 1);
    assert_eq!(terminal["usage"]["provider_cost_nanos"], "1");
    let expiry = first_at + DAY;
    let accounting = delivery.expire(expiry, &terminal).await;
    let mut expected = terminal.clone();
    expected["kind"] = "accounting".into();
    expected.as_object_mut().unwrap().remove("answer");
    assert_eq!(
        accounting, expected,
        "expiry preserves all accounting and drops private text"
    );
    delivery.settle(expiry, &expected).await;
}

struct Delivery {
    rig: Rig,
    reports: mpsc::UnboundedReceiver<Sent>,
    worker: AbortOnDropHandle<crate::Result<()>>,
    settled: Arc<AtomicBool>,
}

impl Delivery {
    fn start() -> Self {
        let settled = Arc::new(AtomicBool::new(false));
        let accepting = Arc::clone(&settled);
        let (sent, reports) = mpsc::unbounded_channel();
        let rig = rig(move |call| match call.verb {
            Verb::AuthoringModel => result(call, &serde_json::json!([]), false),
            Verb::AuthoringReport => {
                let report: Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
                sent.send((Instant::now(), report)).unwrap();
                json(&serde_json::json!({
                    "settled":accepting.load(Ordering::SeqCst), "replayed":true,
                    "sequence":"1", "state":"completed"
                }))
            }
            other => panic!("unexpected authoring authority {other:?}"),
        });
        let lessee = Arc::clone(&rig.lessee);
        let lease = lease();
        let worker = AbortOnDropHandle::new(tokio::spawn(async move {
            lessee.author(&lease, CancellationToken::new()).await
        }));
        Self {
            rig,
            reports,
            worker,
            settled,
        }
    }

    async fn next_report(&mut self) -> Sent {
        tokio::time::timeout(Duration::from_secs(90), async {
            tokio::select! {
                report = self.reports.recv() => report.unwrap(),
                completed = &mut self.worker => panic!("the worker released an unsettled report: {completed:?}"),
            }
        })
        .await
        .unwrap()
    }

    async fn expire(&mut self, expiry: Instant, terminal: &Value) -> Value {
        let mut retries = 0;
        // Visit each retry deadline: a large jump could expire an outstanding
        // native request and would require a reconnect owner.
        let (expired_at, accounting) =
            tokio::time::timeout_at(expiry + STREAM_ANSWER_TIMEOUT, async {
                loop {
                    let (at, report) = self.next_report().await;
                    if report["kind"] == "accounting" {
                        break (at, report);
                    }
                    assert!(
                        at < expiry,
                        "private text was sent after its original expiry"
                    );
                    assert_eq!(
                        &report, terminal,
                        "unsettled retries retain the same report"
                    );
                    retries += 1;
                }
            })
            .await
            .unwrap();
        assert!(retries > 0, "unsettled terminal reports must be retried");
        assert!(expired_at >= expiry, "private expiry must not run early");
        accounting
    }

    async fn settle(mut self, expiry: Instant, expected: &Value) {
        let (at, repeated) = self.next_report().await;
        assert!(at >= expiry);
        assert_eq!(
            &repeated, expected,
            "an unsettled accounting acknowledgement retains the worker"
        );
        self.settled.store(true, Ordering::SeqCst);
        tokio::time::timeout(Duration::from_secs(90), &mut self.worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let (_, accepted) = self.reports.try_recv().unwrap();
        assert_eq!(
            &accepted, expected,
            "settlement retries the same accounting obligation"
        );
        assert!(
            self.reports.try_recv().is_err(),
            "settlement ends report retries"
        );
        assert_eq!(
            self.rig
                .calls()
                .iter()
                .filter(|call| call.verb == Verb::AuthoringModel)
                .count(),
            1,
            "report retention never repeats a physical model call"
        );
        assert!(
            !self
                .rig
                .home
                .sandboxes()
                .parent()
                .unwrap()
                .join("spool")
                .exists()
        );
    }
}
