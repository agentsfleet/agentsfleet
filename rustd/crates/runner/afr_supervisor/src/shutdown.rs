//! The caller grants pending work one lease interval, then every owned worker is joined.

use af_core::{
    error_code,
    timing::{ENGINE_STOP_GRACE, LEASE_TTL},
};
use tokio::task::JoinSet;

use crate::halt::Halt;

pub(crate) async fn finish(halt: &Halt, duties: impl Future<Output = ()>) {
    tokio::pin!(duties);
    tokio::select! {
        biased;
        () = &mut duties => return,
        () = halt.serving().cancelled() => {},
    }
    if !halt.stopped().is_cancelled() {
        if tokio::time::timeout(LEASE_TTL, &mut duties).await.is_ok() {
            return;
        }
        let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
        tracing::warn!(event = "runner_shutdown_grace_expired", error_code);
        halt.stop();
    }
    duties.await;
}

pub(crate) async fn workers(halt: &Halt, workers: &mut JoinSet<()>) {
    let interrupted = {
        let joining = async {
            while let Some(done) = workers.join_next().await {
                if let Err(error) = done {
                    let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
                    tracing::error!(event = "worker_panicked", error_code, error = %error);
                }
            }
        };
        let cleanup = async {
            halt.stopped().cancelled().await;
            tokio::time::sleep(ENGINE_STOP_GRACE).await;
        };
        tokio::select! {
            biased;
            () = joining => false,
            () = cleanup => true,
        }
    };
    if interrupted {
        let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
        let remaining = workers.len();
        tracing::warn!(
            event = "runner_shutdown_cleanup_expired",
            error_code,
            remaining
        );
        workers.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use super::{finish, workers};
    use crate::{halt::Halt, report::delivery::until_acknowledged};
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };
    use tokio::{task::JoinSet, time::Instant};
    use tokio_util::sync::CancellationToken;

    #[tokio::test(start_paused = true)]
    async fn a_report_outage_cannot_hold_caller_shutdown_forever() {
        let shutdown = CancellationToken::new();
        let halt = Halt::new(shutdown.clone());
        let started = Instant::now();
        shutdown.cancel();
        let reporting = async {
            let result = until_acknowledged::<(), _, _>(&halt, std::future::pending).await;
            assert!(
                result.is_err(),
                "the expired grace ends pending memory-only delivery"
            );
        };
        finish(&halt, reporting).await;
        assert_eq!(started.elapsed(), Duration::from_secs(30));
        assert!(halt.stopped().is_cancelled());
    }

    #[tokio::test(start_paused = true)]
    async fn soft_shutdown_delivers_a_report_before_the_grace_without_a_hard_stop() {
        let shutdown = CancellationToken::new();
        let halt = Halt::new(shutdown.clone());
        let started = Instant::now();
        shutdown.cancel();
        let reporting = async {
            let delivered = until_acknowledged(&halt, || async {
                tokio::time::sleep(Duration::from_secs(2)).await;
                Ok(Some("acknowledged"))
            })
            .await;
            assert!(matches!(delivered, Ok("acknowledged")));
        };
        finish(&halt, reporting).await;
        assert_eq!(started.elapsed(), Duration::from_secs(2));
        assert!(!halt.stopped().is_cancelled());
    }

    #[tokio::test(start_paused = true)]
    async fn hard_stop_joins_an_uncooperative_worker_after_cleanup_grace() {
        struct Owned(Arc<AtomicUsize>);
        impl Drop for Owned {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let drops = Arc::new(AtomicUsize::new(0));
        let owned = Owned(Arc::clone(&drops));
        let mut active = JoinSet::new();
        active.spawn(async move {
            let _owned = owned;
            std::future::pending::<()>().await;
        });
        let halt = Halt::new(CancellationToken::new());
        halt.stop();
        let started = Instant::now();
        workers(&halt, &mut active).await;
        assert_eq!(started.elapsed(), Duration::from_secs(5));
        assert!(active.is_empty());
        assert_eq!(
            drops.load(Ordering::SeqCst),
            1,
            "the joined worker released its resources"
        );
    }
}
