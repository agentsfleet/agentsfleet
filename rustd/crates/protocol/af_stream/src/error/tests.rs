//! Stable labels and recovery classes are the contract metrics and fallback read.

use super::{Error, ErrorKind, Reason, Stage};

#[test]
fn every_stage_keeps_its_metric_label_and_operator_sentence() {
    let table = [
        (Stage::Memory, "memory", "reserving message memory"),
        (Stage::Read, "read", "reading a message"),
        (Stage::Write, "write", "writing a message"),
        (Stage::Connect, "connect", "connecting to the server"),
        (
            Stage::Handshake,
            "handshake",
            "verifying the secure connection",
        ),
        (Stage::Open, "open", "opening a stream"),
        (Stage::Close, "close", "stopping connection workers"),
    ];
    for (stage, label, sentence) in table {
        assert_eq!(stage.as_str(), label);
        assert_eq!(stage.to_string(), sentence);
    }
}

#[test]
fn every_reason_keeps_its_metric_label() {
    let table = [
        (Reason::Network, "network"),
        (Reason::Timeout, "timeout"),
        (Reason::Cancelled, "cancelled"),
        (Reason::Protocol, "protocol"),
        (Reason::Closed, "closed"),
        (Reason::Worker, "worker"),
        (Reason::Tls, "tls"),
        (Reason::Unsupported, "unsupported"),
    ];
    for (reason, label) in table {
        assert_eq!(reason.as_str(), label);
    }
}

#[test]
fn quic_failures_separate_tls_refusal_from_network_and_version_loss() {
    use tokio_quiche::quiche::Error as Quic;
    let table = [
        (Quic::TlsFail, Reason::Tls),
        (Quic::CryptoFail, Reason::Tls),
        (Quic::Done, Reason::Network),
        (Quic::UnknownVersion, Reason::Unsupported),
        (Quic::InvalidFrame, Reason::Protocol),
    ];
    for (source, reason) in table {
        assert_eq!(Error::from(source).reason(), reason, "{source:?}");
    }
}

#[test]
fn an_http2_protocol_error_is_never_mistaken_for_network_loss() {
    let protocol = Error::from(h2::Error::from(h2::Reason::PROTOCOL_ERROR));
    assert_eq!(protocol.reason(), Reason::Protocol);
    assert_eq!(protocol.io_kind(), None);
}

#[test]
fn a_quic_handshake_without_an_inner_cause_is_a_network_failure() {
    let failure = Error::from(ErrorKind::QuicHandshake {
        source: std::io::Error::from(std::io::ErrorKind::ConnectionRefused),
    });
    assert_eq!(failure.reason(), Reason::Network);
    assert_eq!(
        failure.io_kind(),
        Some(std::io::ErrorKind::ConnectionRefused)
    );
}

#[tokio::test]
async fn a_failed_worker_is_a_worker_failure_with_the_stable_detail() -> Result<(), &'static str> {
    let worker = tokio::spawn(std::future::pending::<()>());
    worker.abort();
    let failure = Error::from(worker.await.err().ok_or("the aborted worker joined")?);
    assert_eq!(failure.reason(), Reason::Worker);
    assert_eq!(failure.detail(), af_core::error::DETAIL_OPERATION_FAILED);
    Ok(())
}
