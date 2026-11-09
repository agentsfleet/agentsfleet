//! Delivery keeps only the caller's report and observes acknowledgement and cancellation.

use std::sync::atomic::{AtomicUsize, Ordering};

use af_core::error_code;
use tokio_util::sync::CancellationToken;

use super::until_acknowledged;
use crate::{Result, error::ErrorKind, halt::Halt};

#[tokio::test(start_paused = true)]
async fn transient_disconnect_and_pending_settlement_keep_the_same_report() -> Result<()> {
    let halt = Halt::new(CancellationToken::new());
    let attempts = AtomicUsize::new(0);
    let report = zeroize::Zeroizing::new(Vec::from(b"immutable completed report"));
    let address = report.as_ptr();
    until_acknowledged(&halt, || async {
        assert_eq!(report.as_ptr(), address);
        match attempts.fetch_add(1, Ordering::SeqCst) {
            0 => Err(ErrorKind::Disconnected.into()),
            1 => Ok(None),
            _ => Ok(Some(())),
        }
    })
    .await?;
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    Ok(())
}

#[tokio::test]
async fn terminal_refusal_never_retries_with_another_shape() {
    let halt = Halt::new(CancellationToken::new());
    let attempts = AtomicUsize::new(0);
    let result: Result<()> = until_acknowledged(&halt, || async {
        attempts.fetch_add(1, Ordering::SeqCst);
        Err(ErrorKind::StreamRefused {
            code: error_code::INVALID_REQUEST,
            retryable: false,
        }
        .into())
    })
    .await;
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert!(
        matches!(result, Err(failure) if failure.refusal_code() == Some(error_code::INVALID_REQUEST))
    );
}

#[tokio::test]
async fn hard_stop_releases_a_report_waiting_for_a_lost_acknowledgement() {
    let halt = Halt::new(CancellationToken::new());
    let entered = tokio::sync::Notify::new();
    let sending = until_acknowledged::<(), _, _>(&halt, || async {
        entered.notify_one();
        std::future::pending().await
    });
    let stopping = async {
        entered.notified().await;
        halt.stop();
    };
    let (result, ()) = tokio::join!(sending, stopping);
    assert!(result.is_err());
}

/// Stopping work and draining the caller both preserve its final receipt path.
#[tokio::test]
async fn lease_cancellation_and_soft_shutdown_allow_the_final_acknowledgement() -> Result<()> {
    for shutdown in [false, true] {
        let caller = CancellationToken::new();
        let halt = Halt::new(caller.clone());
        if shutdown {
            caller.cancel();
        } else {
            halt.running().cancel();
        }
        let attempts = AtomicUsize::new(0);
        let result = until_acknowledged(&halt, || async {
            attempts.fetch_add(1, Ordering::SeqCst);
            Ok(Some("saved"))
        })
        .await?;
        assert_eq!(result, "saved");
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
    Ok(())
}
