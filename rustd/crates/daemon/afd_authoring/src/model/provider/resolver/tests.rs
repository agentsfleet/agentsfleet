//! Local and numeric hosts exercise the resolver without external DNS or HTTP.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixtures must fail loudly on invalid setup"
)]

use super::*;
use std::{io::ErrorKind, net::SocketAddr};

#[tokio::test]
async fn public_numeric_destination_returns_the_resolved_address() {
    let addresses: Vec<_> = Guarded
        .resolve("1.1.1.1".parse().unwrap())
        .await
        .unwrap()
        .collect();
    assert_eq!(addresses, ["1.1.1.1:0".parse::<SocketAddr>().unwrap()]);
}

#[tokio::test]
async fn private_loopback_and_metadata_destinations_are_refused() {
    for host in [
        "localhost",
        "127.0.0.1",
        "10.0.0.1",
        "192.168.1.1",
        "169.254.169.254",
    ] {
        let result = Guarded.resolve(host.parse().unwrap()).await;
        let failure = result.err().expect("private destination must be refused");
        let failure = failure.downcast_ref::<std::io::Error>().unwrap();
        assert_eq!(failure.kind(), ErrorKind::Other);
        assert_eq!(failure.to_string(), "authoring destination refused");
    }
}

#[tokio::test]
async fn unspecified_destination_is_refused() {
    let result = Guarded.resolve("0.0.0.0".parse().unwrap()).await;
    let failure = result
        .err()
        .expect("unspecified destination must be refused");
    let failure = failure.downcast_ref::<std::io::Error>().unwrap();
    assert_eq!(failure.kind(), ErrorKind::Other);
    assert_eq!(failure.to_string(), "authoring destination refused");
}
