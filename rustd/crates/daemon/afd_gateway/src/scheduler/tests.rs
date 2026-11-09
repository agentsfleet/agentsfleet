//! Real session request queues expose pass order and bounds without a database.

use super::{notifications, pass};
use crate::{
    registry::{Registration, Registry},
    session::SessionHandle,
};
use af_core::id::Uuid7;
use afd_bus::wake::Wake;
use std::{sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

type TestResult<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct SessionFixture {
    handle: Arc<SessionHandle>,
    requests: mpsc::Receiver<oneshot::Sender<bool>>,
}

impl SessionFixture {
    fn new(number: usize) -> TestResult<Self> {
        let id = Uuid7::parse(&format!("019f0000-0000-7000-8000-{number:012x}"))?;
        let (handle, requests) = SessionHandle::fixture_with_claims(id.clone(), id);
        Ok(Self { handle, requests })
    }

    async fn request(&mut self) -> TestResult<oneshot::Sender<bool>> {
        Ok(
            tokio::time::timeout(Duration::from_secs(1), self.requests.recv())
                .await?
                .ok_or("claim queue closed")?,
        )
    }

    fn register(&self, registry: &Arc<Registry>) -> crate::Result<Registration> {
        registry.register_fixture(&self.handle)
    }
}

#[tokio::test(start_paused = true)]
async fn rings_coalesce_into_one_claim_pass() -> TestResult {
    let registry = Arc::new(Registry::default());
    let mut session = SessionFixture::new(1)?;
    let _registration = session.register(&registry)?;
    let wake = Wake::default();
    let stop = CancellationToken::new();
    let job = tokio::spawn({
        let registry = Arc::clone(&registry);
        let wake = wake.clone();
        let stop = stop.clone();
        async move { notifications(&registry, &wake, stop).await }
    });
    tokio::task::yield_now().await;
    wake.ring(session.handle.runner.clone());
    let first = session.request().await?;
    for _ in 0..100 {
        wake.ring(session.handle.runner.clone());
    }
    first
        .send(false)
        .map_err(|unsent| format!("first pass lost its answer {unsent}"))?;
    session
        .request()
        .await?
        .send(false)
        .map_err(|unsent| format!("second pass lost its answer {unsent}"))?;
    let extra = tokio::select! {
        request = session.requests.recv() => {
            assert!(request.is_some(), "fixture request queue stays open");
            true
        }
        () = tokio::time::sleep(Duration::from_secs(1)) => false,
    };
    assert!(
        !extra,
        "rings received during a pass form only one subsequent pass"
    );
    stop.cancel();
    job.await??;
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_one_ring_pass_ends_at_its_first_lease() -> TestResult {
    let mut first = SessionFixture::new(1)?;
    let mut second = SessionFixture::new(2)?;
    let job = tokio::spawn(pass(
        vec![Arc::clone(&first.handle), Arc::clone(&second.handle)],
        1,
    ));
    first
        .request()
        .await?
        .send(true)
        .map_err(|unsent| format!("pass lost its lease answer {unsent}"))?;
    tokio::select! {
        result = job => result??,
        extra = first.requests.recv() => assert!(extra.is_none(), "one ring issued a second claim on its first host"),
        extra = second.requests.recv() => assert!(extra.is_none(), "one ring continued after its first lease"),
    }
    assert!(matches!(
        first.requests.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    assert!(matches!(
        second.requests.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn an_empty_pass_over_five_sessions_makes_five_claims() -> TestResult {
    let mut sessions = (1..=5)
        .map(SessionFixture::new)
        .collect::<TestResult<Vec<_>>>()?;
    let candidates = sessions
        .iter()
        .map(|session| Arc::clone(&session.handle))
        .collect();
    let job = tokio::spawn(pass(candidates, 100));
    let mut claims = 0;
    for session in &mut sessions {
        session
            .request()
            .await?
            .send(false)
            .map_err(|unsent| format!("pass lost its empty answer {unsent}"))?;
        claims += 1;
    }
    job.await??;
    assert_eq!(claims, 5);
    assert!(sessions.iter_mut().all(|session| matches!(
        session.requests.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    )));
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_ring_reaches_a_later_eligible_host_when_the_first_claim_is_empty() -> TestResult {
    let mut first = SessionFixture::new(1)?;
    let mut second = SessionFixture::new(2)?;
    let job = tokio::spawn(pass(
        vec![Arc::clone(&first.handle), Arc::clone(&second.handle)],
        1,
    ));
    first
        .request()
        .await?
        .send(false)
        .map_err(|unsent| format!("pass lost its empty answer {unsent}"))?;
    second
        .request()
        .await?
        .send(true)
        .map_err(|unsent| format!("pass lost its lease answer {unsent}"))?;
    job.await??;
    assert!(matches!(
        second.requests.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn several_rings_spend_one_budget_across_sessions() -> TestResult {
    let mut first = SessionFixture::new(1)?;
    let mut second = SessionFixture::new(2)?;
    let job = tokio::spawn(pass(
        vec![Arc::clone(&first.handle), Arc::clone(&second.handle)],
        3,
    ));
    for _ in 0..2 {
        first
            .request()
            .await?
            .send(true)
            .map_err(|unsent| format!("pass lost its lease answer {unsent}"))?;
    }
    first
        .request()
        .await?
        .send(false)
        .map_err(|unsent| format!("pass lost its empty answer {unsent}"))?;
    second
        .request()
        .await?
        .send(true)
        .map_err(|unsent| format!("pass lost its lease answer {unsent}"))?;
    job.await??;
    assert!(matches!(
        second.requests.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn an_unknown_claim_outcome_stops_the_pass_before_another_host() -> TestResult {
    let mut first = SessionFixture::new(1)?;
    let mut second = SessionFixture::new(2)?;
    let job = tokio::spawn(pass(
        vec![Arc::clone(&first.handle), Arc::clone(&second.handle)],
        1,
    ));
    drop(first.request().await?);
    assert!(
        matches!(job.await?, Err(error) if error.code() == af_core::error_code::INTERNAL_OPERATION_FAILED)
    );
    assert!(matches!(
        second.requests.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn candidates_prefer_locality_then_the_longest_waiting_host() -> TestResult {
    let registry = Arc::new(Registry::default());
    let first = SessionFixture::new(1)?;
    tokio::time::advance(Duration::from_secs(1)).await;
    let second = SessionFixture::new(2)?;
    let local = SessionFixture::new(3)?;
    let _first = first.register(&registry)?;
    let _second = second.register(&registry)?;
    let _local = local.register(&registry)?;
    local
        .handle
        .snapshot
        .lock()
        .held
        .push(local.handle.runner.clone());
    let sessions = registry.candidates(&std::collections::HashSet::from([local
        .handle
        .runner
        .clone()]));
    let order: Vec<_> = sessions
        .into_iter()
        .map(|session| session.runner.clone())
        .collect();
    assert_eq!(
        order,
        [
            local.handle.runner.clone(),
            first.handle.runner.clone(),
            second.handle.runner.clone()
        ]
    );
    first.handle.request_offer();
    let oldest = registry.candidates(&std::collections::HashSet::new());
    assert_eq!(
        oldest.first().map(|session| &session.runner),
        Some(&first.handle.runner)
    );
    Ok(())
}

#[tokio::test]
async fn draining_or_superseded_sessions_are_not_scheduled() -> TestResult {
    let registry = Arc::new(Registry::default());
    let draining = SessionFixture::new(1)?;
    let superseded = SessionFixture::new(2)?;
    let eligible = SessionFixture::new(3)?;
    let _draining = draining.register(&registry)?;
    let _superseded = superseded.register(&registry)?;
    let _eligible = eligible.register(&registry)?;
    draining.handle.drain.cancel();
    superseded.handle.supersede();
    let sessions = registry.candidates(&std::collections::HashSet::new());
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions.first().map(|session| &session.runner),
        Some(&eligible.handle.runner)
    );
    Ok(())
}
