#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use afd_core::id::Uuid7;
use tokio::sync::Notify;

use super::super::{
    Account, Attribution, Balance, Biller, Hold, Receipt, Release, Reservation, Settlement, WorkKey,
};
use super::Queued;
use crate::error::ErrorKind;
use crate::{Nanos, Posture, Result};

/// A biller that records each movement, can hold movements until opened, and
/// can fail its first few calls.
#[derive(Debug, Default)]
struct Ledger {
    moved: Mutex<Vec<&'static str>>,
    gate: Option<Arc<Notify>>,
    failures_left: Mutex<u32>,
    short: bool,
    permanent: bool,
}

impl Ledger {
    async fn movement(&self, name: &'static str) -> Result<()> {
        if let Some(gate) = &self.gate {
            gate.notified().await;
        }
        {
            let mut left = self.failures_left.lock().unwrap();
            if *left > 0 {
                *left -= 1;
                return Err(self.fault());
            }
        }
        self.moved.lock().unwrap().push(name);
        Ok(())
    }

    /// An outage, which the queue retries, unless the ledger was built to refuse for good.
    fn fault(&self) -> crate::Error {
        if self.permanent {
            return ErrorKind::WalletMissing.into();
        }
        ErrorKind::Datastore {
            source: afd_db::error::invalid_bool_knob("QUEUED_TEST"),
        }
        .into()
    }

    fn moved(&self) -> Vec<&'static str> {
        self.moved.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Biller for Ledger {
    async fn reserve(&self, _ask: &Reservation) -> Result<Hold> {
        self.movement("reserve").await?;
        Ok(if self.short {
            Hold::Short {
                available: Nanos::ZERO,
                asked: Nanos::ZERO,
            }
        } else {
            Hold::Held {
                ceiling: Nanos::ZERO,
                captured: Nanos::ZERO,
            }
        })
    }

    async fn settle(&self, _settlement: &Settlement) -> Result<Receipt> {
        self.movement("settle").await?;
        Ok(Receipt::default())
    }

    async fn release(&self, _work: &WorkKey, _why: Release) -> Result<()> {
        self.movement("release").await
    }

    async fn balance(&self, _payer: &Account) -> Result<Balance> {
        Err(ErrorKind::WalletMissing.into())
    }
}

fn work() -> WorkKey {
    WorkKey::AuthoringAttempt(Uuid7::parse("01a1a78b-a0ef-7c8c-9013-b63811502ab1").unwrap())
}

fn payer() -> Account {
    let id = Uuid7::parse("01a1a752-f928-7cda-b99e-9a119bcc4e88").unwrap();
    Account {
        tenant: id.clone(),
        workspace: id,
    }
}

fn reservation() -> Reservation {
    Reservation {
        work: work(),
        payer: payer(),
        ceiling: Nanos::ZERO,
    }
}

fn settlement() -> Settlement {
    Settlement {
        work: work(),
        payer: payer(),
        attribution: Attribution::SetupChat,
        provider: "provider".into(),
        model: "model".into(),
        posture: Posture::Platform,
        lines: Vec::new(),
    }
}

/// Waits until `ledger` has moved `expected`, or fails after a generous bound.
async fn eventually(ledger: &Ledger, expected: &[&str]) {
    for _ in 0..200 {
        if ledger.moved() == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the ledger moved {:?}, not {expected:?}", ledger.moved());
}

#[tokio::test]
async fn settle_and_release_answer_before_the_ledger_moves_then_run_in_order() {
    let gate = Arc::new(Notify::new());
    let ledger = Arc::new(Ledger {
        gate: Some(Arc::clone(&gate)),
        ..Ledger::default()
    });
    let queued = Queued::in_memory(Arc::clone(&ledger) as Arc<dyn Biller>);

    let receipt = queued.settle(&settlement()).await.unwrap();
    queued.release(&work(), Release::Completed).await.unwrap();

    assert_eq!(
        receipt,
        Receipt::default(),
        "a queued settlement promises nothing yet"
    );
    assert!(ledger.moved().is_empty(), "nothing waited for the ledger");
    gate.notify_one();
    tokio::time::sleep(Duration::from_millis(20)).await;
    gate.notify_one();
    eventually(&ledger, &["settle", "release"]).await;
}

#[tokio::test]
async fn reserve_still_waits_so_a_short_balance_stops_the_run() {
    let ledger = Arc::new(Ledger {
        short: true,
        ..Ledger::default()
    });
    let queued = Queued::in_memory(Arc::clone(&ledger) as Arc<dyn Biller>);

    let hold = queued.reserve(&reservation()).await.unwrap();

    assert!(matches!(hold, Hold::Short { .. }), "{hold:?}");
    assert!(!hold.admits());
    assert_eq!(ledger.moved(), ["reserve"]);
}

#[tokio::test]
async fn a_detached_hold_answers_unconfirmed_and_the_worker_places_it() {
    let ledger = Arc::new(Ledger {
        short: true,
        ..Ledger::default()
    });
    let queued = Queued::in_memory(Arc::clone(&ledger) as Arc<dyn Biller>);

    let hold = queued.reserve_detached(&reservation()).await.unwrap();

    assert_eq!(hold, Hold::Unconfirmed);
    eventually(&ledger, &["reserve"]).await;
}

#[tokio::test]
async fn a_job_the_biller_could_not_answer_is_retried_until_it_lands() {
    let ledger = Arc::new(Ledger {
        failures_left: Mutex::new(2),
        ..Ledger::default()
    });
    let queued = Queued::in_memory(Arc::clone(&ledger) as Arc<dyn Biller>);

    queued.settle(&settlement()).await.unwrap();

    eventually(&ledger, &["settle"]).await;
    assert_eq!(
        *ledger.failures_left.lock().unwrap(),
        0,
        "both failures were retried"
    );
}

#[test]
fn a_refused_job_is_left_to_the_sweeper_and_never_moved_out_of_order() {
    let ledger = Arc::new(Ledger::default());
    let queued = Queued::in_memory(Arc::clone(&ledger) as Arc<dyn Biller>);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    let receipt = runtime.block_on(async {
        queued.reserve_detached(&reservation()).await.unwrap();
        queued.release(&work(), Release::Completed).await.unwrap();
        queued.settle(&settlement()).await.unwrap()
    });

    assert_eq!(receipt, Receipt::default());
    assert_eq!(
        ledger.moved(),
        Vec::<&str>::new(),
        "no queue, so nothing moves beside it"
    );
}

#[tokio::test]
async fn a_plain_biller_waits_for_a_detached_hold_like_any_reserve() {
    let ledger = Ledger {
        short: true,
        ..Ledger::default()
    };

    let hold = ledger.reserve_detached(&reservation()).await.unwrap();

    assert!(
        matches!(hold, Hold::Short { .. }),
        "the default answers what reserve answers"
    );
}

#[tokio::test]
async fn a_permanent_refusal_is_not_retried_and_moves_nothing() {
    let ledger = Arc::new(Ledger {
        failures_left: Mutex::new(1),
        permanent: true,
        ..Ledger::default()
    });
    let queued = Queued::in_memory(Arc::clone(&ledger) as Arc<dyn Biller>);

    queued.settle(&settlement()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(
        *ledger.failures_left.lock().unwrap(),
        0,
        "the one refusal was seen"
    );
    assert_eq!(
        ledger.moved(),
        Vec::<&str>::new(),
        "a refusal is left to the sweeper, not retried"
    );
}
