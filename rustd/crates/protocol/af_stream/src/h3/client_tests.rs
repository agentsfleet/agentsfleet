//! Receive-task failures remain distinguishable from caller cancellation during setup.

use super::receiving;
use crate::{Reason, Result, error::ErrorKind};
use std::{io::ErrorKind as IoKind, time::Duration};
use tokio::time::Instant;
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

#[tokio::test]
async fn a_receive_error_during_handshake_retains_its_network_cause() -> Result<()> {
    let parent = CancellationToken::new();
    let native = parent.child_token();
    let worker_stop = native.clone();
    let mut receive = AbortOnDropHandle::new(tokio::spawn(async move {
        let _guard = worker_stop.drop_guard();
        Err(std::io::Error::from(IoKind::ConnectionReset).into())
    }));
    let failure = receiving::<()>(
        &parent,
        Instant::now() + Duration::from_secs(1),
        &mut receive,
        std::future::pending(),
    )
    .await;
    let failure = failure.err().ok_or(ErrorKind::Protocol)?;
    assert!(native.is_cancelled());
    assert!(!parent.is_cancelled());
    assert_eq!(failure.reason(), Reason::Network);
    assert_eq!(failure.io_kind(), Some(IoKind::ConnectionReset));
    assert!(std::error::Error::source(&failure).is_some());
    Ok(())
}

#[tokio::test]
async fn cancellation_of_the_caller_is_still_cancellation() -> Result<()> {
    let parent = CancellationToken::new();
    let native = parent.child_token();
    let worker_stop = native.clone();
    let mut receive = AbortOnDropHandle::new(tokio::spawn(async move {
        worker_stop.cancelled().await;
        Ok(())
    }));
    parent.cancel();
    let failure = receiving::<()>(
        &parent,
        Instant::now() + Duration::from_secs(1),
        &mut receive,
        std::future::pending(),
    )
    .await;
    assert!(matches!(failure, Err(ref error) if error.reason() == Reason::Cancelled));
    receive.await??;
    Ok(())
}

#[tokio::test]
async fn a_ready_identity_or_protocol_refusal_is_not_reclassified_as_a_receive_failure()
-> Result<()> {
    for refusal in [
        ErrorKind::KeyExchangeRefused {
            negotiated: crate::tls::KeyExchange::X25519,
        },
        ErrorKind::Protocol,
    ] {
        let parent = CancellationToken::new();
        let mut receive = AbortOnDropHandle::new(tokio::spawn(async {
            Err(std::io::Error::from(IoKind::ConnectionReset).into())
        }));
        let expected = crate::Error::from(refusal);
        let reason = expected.reason();
        let failure = receiving::<()>(
            &parent,
            Instant::now() + Duration::from_secs(1),
            &mut receive,
            async { Err(expected) },
        )
        .await;
        assert!(matches!(failure, Err(ref error) if error.reason() == reason));
    }
    Ok(())
}
