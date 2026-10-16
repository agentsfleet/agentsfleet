//! A background fake reports its bugs to the test that owns its call log.

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use tokio::sync::mpsc;

use super::Answer;
use crate::client::{Call, ControlPlane, RunnerApi};
use crate::error;

const TEST_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn bounded<T>(operation: &str, future: impl Future<Output = T>) -> T {
    tokio::time::timeout(TEST_TIMEOUT, future)
        .await
        .unwrap_or_else(|_| panic!("timed out after {TEST_TIMEOUT:?} waiting for {operation}"))
}

pub(crate) struct Calls {
    received: mpsc::UnboundedReceiver<Call>,
    failures: Arc<Mutex<Vec<String>>>,
}

impl Calls {
    pub(crate) async fn recv(&mut self) -> Option<Call> {
        self.received.recv().await
    }

    fn check(&self) {
        let failures = self.failures.lock().unwrap().clone();
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
    failures: Arc<Mutex<Vec<String>>>,
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
                self.failures
                    .lock()
                    .unwrap()
                    .push(format!("{} {}: {message}", call.verb, call.path));
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
    (ControlPlane::new(Box::new(api)), calls)
}

fn fake<F>(answer: F) -> (FakeApi<F>, Calls)
where
    F: Fn(&Call) -> Answer + Send + Sync,
{
    super::fixtures::validate();
    let (calls, received) = mpsc::unbounded_channel();
    let failures = Arc::default();
    let api = FakeApi {
        answer,
        calls,
        failures: Arc::clone(&failures),
    };
    (api, Calls { received, failures })
}

pub(crate) fn drain(calls: &mut Calls) -> Vec<Call> {
    calls.check();
    std::iter::from_fn(|| calls.received.try_recv().ok()).collect()
}

mod tests;
