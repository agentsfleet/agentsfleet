//! A provider held at its network boundary until every competing caller is polled.
#![expect(clippy::expect_used, reason = "test prerequisites must fail loudly")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use afd_auth::capability::CapabilitySource;
use afd_auth::error::Unavailable;
use afd_auth::principal::Subject;
use afd_auth::scope::ScopeSet;
use afd_core::clock::{Clock, FixedClock, UnixMillis};
use afd_identity::ProviderCapabilities;
use afd_identity::capability::ClaimSource;
use afd_identity::error::ClaimUnavailable;
use tokio::sync::Semaphore;

pub(crate) const CLAIM: &str = "fleet:admin billing:read";
pub(crate) const CALLERS: u32 = 100;
const INITIAL_CLOCK_MS: i64 = 1_000_000;

#[derive(Debug)]
pub(crate) struct ClaimProvider {
    answer: Mutex<Result<String, ClaimUnavailable>>,
    pub(crate) calls: AtomicUsize,
    pub(crate) release: Semaphore,
}

impl ClaimProvider {
    pub(crate) fn answer(&self, answer: Result<String, ClaimUnavailable>) {
        *self
            .answer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = answer;
    }
}

impl ClaimSource for ClaimProvider {
    async fn claim(&self, _subject: &Subject) -> Result<String, ClaimUnavailable> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let answer = self
            .answer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        self.release
            .acquire()
            .await
            .expect("the provider remains open")
            .forget();
        answer
    }
}

pub(crate) fn subject() -> Subject {
    Subject::new("user_cached").expect("a nonblank subject")
}

pub(crate) fn resolver() -> (ProviderCapabilities<ClaimProvider>, Arc<FixedClock>) {
    let clock = Arc::new(FixedClock::at(UnixMillis::from_millis(INITIAL_CLOCK_MS)));
    let source = ClaimProvider {
        answer: Mutex::new(Ok(CLAIM.to_owned())),
        calls: AtomicUsize::new(0),
        release: Semaphore::new(0),
    };
    (
        ProviderCapabilities::new(source, Arc::clone(&clock) as Arc<dyn Clock>),
        clock,
    )
}

pub(crate) async fn warm(resolver: &ProviderCapabilities<ClaimProvider>) {
    resolver.source().release.add_permits(1);
    resolver
        .capabilities(&subject())
        .await
        .expect("the initial claim is available");
}

pub(crate) async fn wave(
    resolver: &ProviderCapabilities<ClaimProvider>,
    distinct: bool,
    expected_calls: usize,
) -> Vec<Result<ScopeSet, Unavailable>> {
    let started = Arc::new(Semaphore::new(0));
    let mut tasks = tokio::task::JoinSet::new();
    for index in 0..CALLERS {
        let resolver = resolver.clone();
        let started = Arc::clone(&started);
        tasks.spawn(async move {
            let who = if distinct {
                Subject::new(&format!("user_{index}")).expect("a distinct subject")
            } else {
                subject()
            };
            let mut request = std::pin::pin!(resolver.capabilities(&who));
            let mut first_poll = true;
            std::future::poll_fn(|context| {
                let result = request.as_mut().poll(context);
                if first_poll {
                    first_poll = false;
                    started.add_permits(1);
                }
                result
            })
            .await
        });
    }
    started
        .acquire_many(CALLERS)
        .await
        .expect("every request was polled")
        .forget();
    assert_eq!(
        resolver.source().calls.load(Ordering::SeqCst),
        expected_calls
    );
    resolver
        .source()
        .release
        .add_permits(if distinct { CALLERS as usize } else { 1 });
    let mut answers = Vec::new();
    while let Some(answer) = tasks.join_next().await {
        answers.push(answer.expect("the request task completes"));
    }
    answers
}
