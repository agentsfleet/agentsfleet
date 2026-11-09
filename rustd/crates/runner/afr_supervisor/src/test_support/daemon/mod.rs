//! A background fake reports its bugs to the test that owns its call log.

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use bytes::Bytes;
use tokio::sync::mpsc;

use super::Answer;
use crate::client::{Call, ControlPlane, RunnerApi};
use crate::error;

mod stream;

#[derive(Debug)]
struct SharedApi(std::sync::Arc<dyn RunnerApi>);

#[async_trait::async_trait]
impl RunnerApi for SharedApi {
    async fn send(&self, call: Call) -> crate::Result<Bytes> {
        self.0.send(call).await
    }
}

const TEST_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn bounded<T>(operation: &str, future: impl Future<Output = T>) -> T {
    tokio::time::timeout(TEST_TIMEOUT, future)
        .await
        .unwrap_or_else(|_| panic!("timed out after {TEST_TIMEOUT:?} waiting for {operation}"))
}

pub(crate) struct Calls {
    pub(crate) stopped: tokio_util::sync::CancellationToken,
    received: mpsc::UnboundedReceiver<Call>,
    /// Each answer that panicked, as the fake daemon reports it.
    failures: mpsc::UnboundedReceiver<String>,
}

impl Calls {
    pub(crate) async fn recv(&mut self) -> Option<Call> {
        self.received.recv().await
    }

    fn check(&mut self) {
        let failures: Vec<String> = std::iter::from_fn(|| self.failures.try_recv().ok()).collect();
        assert!(failures.is_empty(), "fake daemon failed: {failures:?}");
    }
}

impl Drop for Calls {
    fn drop(&mut self) {
        // Preserve the original assertion when unwinding a failed test.
        if !std::thread::panicking() {
            self.check();
        }
    }
}

struct FakeApi<F> {
    answer: F,
    calls: mpsc::UnboundedSender<Call>,
    failures: mpsc::UnboundedSender<String>,
}

impl<F> fmt::Debug for FakeApi<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FakeApi")
    }
}

#[async_trait::async_trait]
impl<F> RunnerApi for FakeApi<F>
where
    F: Fn(&Call) -> Answer + Send + Sync,
{
    async fn send(&self, call: Call) -> crate::Result<Bytes> {
        let answer =
            catch_unwind(AssertUnwindSafe(|| (self.answer)(&call))).unwrap_or_else(|panic| {
                let message = panic
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| panic.downcast_ref::<&str>().copied())
                    .unwrap_or("non-string panic");
                drop(
                    self.failures
                        .send(format!("{} {}: {message}", call.verb, call.path)),
                );
                Answer::Fail(error::unavailable(call.verb, 500))
            });
        drop(self.calls.send(call));
        match answer {
            Answer::Reply(bytes) => Ok(bytes),
            Answer::Fail(failure) => Err(failure),
            Answer::Stall => std::future::pending().await,
            Answer::Late(after, bytes) => {
                tokio::time::sleep(after).await;
                Ok(bytes)
            }
        }
    }
}

pub(crate) fn plane<F>(answer: F) -> (ControlPlane, Calls)
where
    F: Fn(&Call) -> Answer + Send + Sync + 'static,
{
    let (api, calls) = fake(answer);
    let api: std::sync::Arc<dyn RunnerApi> = std::sync::Arc::new(api);
    let stream = stream::start(&api, calls.stopped.clone());
    (ControlPlane::new(Box::new(SharedApi(api)), stream), calls)
}

/// Task HTTP boundaries use a real client while session operations keep their scripted frame path.
pub(crate) fn plane_with_http<F>(api: Box<dyn RunnerApi>, answer: F) -> (ControlPlane, Calls)
where
    F: Fn(&Call) -> Answer + Send + Sync + 'static,
{
    let (native, calls) = plane(answer);
    (ControlPlane::new(api, native.stream), calls)
}

/// A controllable batch writer keeps batching tests independent of asynchronous native delivery.
pub(crate) fn activities<F>(answer: F) -> (impl crate::activity::ActivityWriter, Calls)
where
    F: Fn(&Call) -> Answer + Send + Sync,
{
    let (api, calls) = fake(answer);
    (ActivityWrites(api), calls)
}

struct ActivityWrites<F>(FakeApi<F>);

impl<F> crate::activity::ActivityWriter for ActivityWrites<F>
where
    F: Fn(&Call) -> Answer + Send + Sync,
{
    async fn activity(
        &self,
        _lease: &af_core::id::Uuid7,
        request: &af_wire::activity::ActivityRequest<'_>,
    ) -> crate::Result<()> {
        self.0
            .send(Call {
                verb: crate::client::Verb::Activity,
                path: std::borrow::Cow::Borrowed(af_wire::paths::RUNNER_STREAM),
                body: Some(bytes::Bytes::from(
                    serde_json::to_vec(request).map_err(error::encode)?,
                )),
            })
            .await?;
        Ok(())
    }
}

fn fake<F>(answer: F) -> (FakeApi<F>, Calls)
where
    F: Fn(&Call) -> Answer + Send + Sync,
{
    super::fixtures::validate();
    let (calls, received) = mpsc::unbounded_channel();
    let (failed, failures) = mpsc::unbounded_channel();
    let api = FakeApi {
        answer,
        calls,
        failures: failed,
    };
    (
        api,
        Calls {
            received,
            failures,
            stopped: tokio_util::sync::CancellationToken::new(),
        },
    )
}

pub(crate) fn drain(calls: &mut Calls) -> Vec<Call> {
    calls.check();
    std::iter::from_fn(|| calls.received.try_recv().ok()).collect()
}

mod tests;
