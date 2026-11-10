//! The replica clock runs independently of host count and notification traffic.

use super::probe_ticks;
use crate::{
    registry::{Registration, Registry},
    scheduler::notifications,
    session::SessionHandle,
};
use af_core::{id::Uuid7, timing::OFFER_FALLBACK_TICK};
use afd_bus::wake::Wake;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

type TestResult<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct ReplicaFixture {
    registry: Arc<Registry>,
    requests: Vec<mpsc::Receiver<oneshot::Sender<bool>>>,
    registrations: Vec<Registration>,
}

impl ReplicaFixture {
    fn new() -> TestResult<Self> {
        let registry = Arc::new(Registry::default());
        let mut requests = Vec::new();
        let mut registrations = Vec::new();
        for number in 1..=50 {
            let id = Uuid7::parse(&format!("019f0000-0000-7000-8000-{number:012x}"))?;
            let (session, claims) = SessionHandle::fixture_with_claims(id.clone(), id);
            registrations.push(registry.register_fixture(&session)?);
            requests.push(claims);
        }
        Ok(Self {
            registry,
            requests,
            registrations,
        })
    }
}

#[tokio::test(start_paused = true)]
async fn the_fallback_tick_probes_once_per_replica() -> TestResult {
    let mut fixture = ReplicaFixture::new()?;
    let wake = Wake::default();
    let stop = CancellationToken::new();
    let pending = Arc::new(AtomicBool::new(false));
    let (calls, mut observed) = watch::channel(0);
    let clock = start_clock(wake.clone(), stop.clone(), Arc::clone(&pending), calls);
    let scheduler = tokio::spawn({
        let registry = Arc::clone(&fixture.registry);
        let stop = stop.clone();
        async move { notifications(&registry, &wake, stop).await }
    });
    tokio::task::yield_now().await;
    for expected in 1..=3 {
        tokio::time::advance(OFFER_FALLBACK_TICK).await;
        observed.changed().await?;
        assert_eq!(*observed.borrow_and_update(), expected);
    }
    assert!(
        fixture
            .requests
            .iter_mut()
            .all(|requests| matches!(requests.try_recv(), Err(mpsc::error::TryRecvError::Empty)))
    );
    pending.store(true, Ordering::SeqCst);
    tokio::time::advance(OFFER_FALLBACK_TICK).await;
    observed.changed().await?;
    assert_eq!(*observed.borrow_and_update(), 4);
    let replies = futures_util::future::try_join_all(fixture.requests.iter_mut().map(
        |requests| async move {
            let answer = requests.recv().await.ok_or("fallback pass lost a host")?;
            answer
                .send(false)
                .map_err(|unsent| format!("fallback pass lost its empty answer {unsent}"))
        },
    ));
    let answered = tokio::time::timeout(OFFER_FALLBACK_TICK, replies).await??;
    assert_eq!(answered.len(), 50);
    stop.cancel();
    clock.await?;
    scheduler.await??;
    drop(fixture.registrations);
    Ok(())
}

fn start_clock(
    wake: Wake,
    stop: CancellationToken,
    pending: Arc<AtomicBool>,
    calls: watch::Sender<usize>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        probe_ticks(&wake, &stop, || async {
            calls.send_modify(|count| *count += 1);
            Ok(pending.load(Ordering::SeqCst))
        })
        .await;
    })
}
