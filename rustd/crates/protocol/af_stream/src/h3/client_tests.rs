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
        &native,
        &tokio::sync::watch::channel(None).1,
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
        &native,
        &tokio::sync::watch::channel(None).1,
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
        ErrorKind::QuicHandshake {
            source: std::io::Error::other(tokio_quiche::quiche::Error::TlsFail),
        },
        ErrorKind::QuicHandshake {
            source: std::io::Error::other(tokio_quiche::quiche::Error::CryptoFail),
        },
        ErrorKind::QuicHandshake {
            source: std::io::Error::other(tokio_quiche::quiche::Error::InvalidFrame),
        },
        ErrorKind::QuicHandshake {
            source: std::io::Error::other(tokio_quiche::quiche::Error::UnknownVersion),
        },
    ] {
        let parent = CancellationToken::new();
        let native = parent.child_token();
        let worker_stop = native.clone();
        let mut receive = AbortOnDropHandle::new(tokio::spawn(async move {
            let _guard = worker_stop.drop_guard();
            Err(std::io::Error::from(IoKind::ConnectionReset).into())
        }));
        native.cancelled().await;
        let expected = crate::Error::from(refusal);
        let reason = expected.reason();
        let io_kind = expected.io_kind();
        let failure = receiving::<()>(
            &parent,
            &native,
            &tokio::sync::watch::channel(None).1,
            Instant::now() + Duration::from_secs(1),
            &mut receive,
            async { Err(expected) },
        )
        .await;
        assert!(matches!(failure, Err(ref error) if error.reason() == reason));
        assert!(matches!(failure, Err(ref error) if error.io_kind() == io_kind));
    }
    Ok(())
}

#[tokio::test]
async fn a_ready_erased_handshake_error_cannot_hide_the_receive_network_cause() -> Result<()> {
    let parent = CancellationToken::new();
    let native = parent.child_token();
    let worker_stop = native.clone();
    let mut receive = AbortOnDropHandle::new(tokio::spawn(async move {
        let _guard = worker_stop.drop_guard();
        Err(std::io::Error::from(IoKind::ConnectionReset).into())
    }));
    native.cancelled().await;
    assert!(receive.is_finished());
    let failure = receiving::<()>(
        &parent,
        &native,
        &tokio::sync::watch::channel(None).1,
        Instant::now() + Duration::from_secs(1),
        &mut receive,
        async {
            Err(ErrorKind::QuicHandshake {
                source: std::io::Error::other("erased native worker failure"),
            }
            .into())
        },
    )
    .await
    .err()
    .ok_or(ErrorKind::Protocol)?;
    assert_eq!(failure.reason(), Reason::Network);
    assert_eq!(failure.io_kind(), Some(IoKind::ConnectionReset));
    assert!(!parent.is_cancelled());
    Ok(())
}

#[tokio::test]
async fn an_erased_handshake_is_not_network_loss_when_the_receiver_succeeded() -> Result<()> {
    let parent = CancellationToken::new();
    let native = parent.child_token();
    let worker_stop = native.clone();
    let mut receive = AbortOnDropHandle::new(tokio::spawn(async move {
        let _guard = worker_stop.drop_guard();
        Ok(())
    }));
    native.cancelled().await;
    assert!(receive.is_finished());
    let failure = receiving::<()>(
        &parent,
        &native,
        &tokio::sync::watch::channel(None).1,
        Instant::now() + Duration::from_secs(1),
        &mut receive,
        async {
            Err(ErrorKind::QuicHandshake {
                source: std::io::Error::other("erased native worker failure"),
            }
            .into())
        },
    )
    .await
    .err()
    .ok_or(ErrorKind::Protocol)?;
    assert_eq!(failure.reason(), Reason::Tls);
    assert_eq!(failure.io_kind(), Some(IoKind::Other));
    Ok(())
}

#[tokio::test]
async fn a_live_native_worker_does_not_replace_an_erased_handshake_error() -> Result<()> {
    let parent = CancellationToken::new();
    let native = parent.child_token();
    let ready = CancellationToken::new();
    let worker_ready = ready.clone();
    let mut receive = AbortOnDropHandle::new(tokio::spawn(async move {
        let _guard = worker_ready.drop_guard();
        Err(std::io::Error::from(IoKind::ConnectionReset).into())
    }));
    ready.cancelled().await;
    assert!(receive.is_finished());
    let failure = receiving::<()>(
        &parent,
        &native,
        &tokio::sync::watch::channel(None).1,
        Instant::now() + Duration::from_secs(1),
        &mut receive,
        async {
            Err(ErrorKind::QuicHandshake {
                source: std::io::Error::other("erased native worker failure"),
            }
            .into())
        },
    )
    .await
    .err()
    .ok_or(ErrorKind::Protocol)?;
    assert_eq!(failure.reason(), Reason::Tls);
    assert!(!native.is_cancelled());
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn receiver_reconciliation_keeps_the_original_deadline() -> Result<()> {
    let parent = CancellationToken::new();
    let native = parent.child_token();
    native.cancel();
    let mut receive = AbortOnDropHandle::new(tokio::spawn(std::future::pending()));
    let deadline = Instant::now() + Duration::from_secs(1);
    let failure = receiving::<()>(
        &parent,
        &native,
        &tokio::sync::watch::channel(None).1,
        deadline,
        &mut receive,
        async {
            Err(ErrorKind::QuicHandshake {
                source: std::io::Error::other("erased native worker failure"),
            }
            .into())
        },
    )
    .await
    .err()
    .ok_or(ErrorKind::Protocol)?;
    assert_eq!(failure.reason(), Reason::Timeout);
    assert_eq!(Instant::now(), deadline);
    Ok(())
}

#[tokio::test]
async fn caller_cancellation_interrupts_receiver_reconciliation() -> Result<()> {
    let parent = CancellationToken::new();
    let native = parent.child_token();
    native.cancel();
    let mut receive = AbortOnDropHandle::new(tokio::spawn(std::future::pending()));
    let failure = receiving::<()>(
        &parent,
        &native,
        &tokio::sync::watch::channel(None).1,
        Instant::now() + Duration::from_secs(1),
        &mut receive,
        async {
            parent.cancel();
            Err(ErrorKind::QuicHandshake {
                source: std::io::Error::other("erased native worker failure"),
            }
            .into())
        },
    )
    .await
    .err()
    .ok_or(ErrorKind::Protocol)?;
    assert_eq!(failure.reason(), Reason::Cancelled);
    Ok(())
}

#[tokio::test]
async fn an_erased_post_negotiation_refusal_cannot_become_network_loss() -> Result<()> {
    for group in [
        crate::tls::KeyExchange::X25519,
        crate::tls::KeyExchange::X25519MlKem768,
    ] {
        let parent = CancellationToken::new();
        let native = parent.child_token();
        let worker_stop = native.clone();
        let mut receive = AbortOnDropHandle::new(tokio::spawn(async move {
            let _guard = worker_stop.drop_guard();
            Err(std::io::Error::from(IoKind::ConnectionReset).into())
        }));
        let (_published, security) = tokio::sync::watch::channel(Some(group));
        native.cancelled().await;
        assert_eq!(*security.borrow(), Some(group));
        let failure = receiving::<()>(
            &parent,
            &native,
            &security,
            Instant::now() + Duration::from_secs(1),
            &mut receive,
            async {
                Err(ErrorKind::QuicHandshake {
                    source: std::io::Error::other("erased post-negotiation refusal"),
                }
                .into())
            },
        )
        .await
        .err()
        .ok_or(ErrorKind::Protocol)?;
        assert_eq!(failure.reason(), Reason::Tls);
        assert_eq!(failure.io_kind(), Some(IoKind::Other));
    }
    Ok(())
}
