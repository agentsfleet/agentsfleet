//! Real TLS peers refuse classical-only key agreement and obsolete protocol versions.

use std::{path::Path, time::Duration};

use boring::ssl::{SslAcceptor, SslFiletype, SslMethod, SslVersion};
use tokio::net::{TcpListener, TcpStream};

use af_observability::metrics::label::fleet::{StreamKeyExchange, StreamPeer};

use super::KeyExchange;
use crate::{Reason, Transport, tls::Trust};

type TestResult<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::test]
async fn connector_refuses_every_classical_only_tls13_peer() -> TestResult {
    for group in ["X25519", "P-256", "P-384"] {
        let Err(_refused) = negotiate(group, SslVersion::TLS1_3).await else {
            return Err(format!("classical-only peer {group} was accepted").into());
        };
    }
    Ok(())
}

#[tokio::test]
async fn connector_and_acceptor_negotiate_the_required_hybrid() -> TestResult {
    let (client, server) = negotiate(super::REQUIRED_GROUP, SslVersion::TLS1_3).await?;
    assert_eq!(client, KeyExchange::X25519MlKem768);
    assert_eq!(server, KeyExchange::X25519MlKem768);
    Ok(())
}

#[tokio::test]
async fn daemon_acceptor_refuses_an_x25519_only_client() -> TestResult {
    let directory = tempfile::tempdir()?;
    let _files = peer(directory.path(), super::REQUIRED_GROUP, SslVersion::TLS1_3)?;
    let identity = crate::tls::Identity::new(
        directory.path().join("certificate.pem"),
        directory.path().join("key.pem"),
    );
    let acceptor = identity.h2()?;
    let mut connector = Trust::with_roots(directory.path().join("certificate.pem"))
        .h2()?
        .configure()?;
    connector.set_curves_list("X25519")?;
    let listener = TcpListener::bind("[::1]:0").await?;
    let address = listener.local_addr()?;
    let server = async {
        let (socket, _peer) = listener.accept().await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
            tokio_boring::accept(&acceptor, socket).await.is_err(),
        )
    };
    let client = async {
        let socket = TcpStream::connect(address).await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
            tokio_boring::connect(connector, "localhost", socket)
                .await
                .is_err(),
        )
    };
    let (client_refused, server_refused) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::try_join!(client, server)
    })
    .await??;
    assert!(client_refused && server_refused);
    Ok(())
}

#[tokio::test]
async fn a_tls12_only_peer_is_refused_instead_of_lowering_the_protocol_minimum() -> TestResult {
    let Err(_unsupported) = negotiate("X25519", SslVersion::TLS1_2).await else {
        return Err("TLS 1.2 was accepted".into());
    };
    Ok(())
}

#[test]
fn an_unnegotiated_ssl_object_cannot_be_reported_as_post_quantum() -> TestResult {
    let context = boring::ssl::SslContextBuilder::new(SslMethod::tls())?.build();
    let ssl = boring::ssl::Ssl::new(&context)?;
    let group = KeyExchange::from(ssl.as_ref());
    assert_eq!(group, KeyExchange::Other);
    assert_eq!(group.as_str(), "other");
    Ok(())
}

async fn negotiate(group: &str, version: SslVersion) -> TestResult<(KeyExchange, KeyExchange)> {
    let directory = tempfile::tempdir()?;
    let acceptor = peer(directory.path(), group, version)?;
    let trust = Trust::with_roots(directory.path().join("certificate.pem"));
    let connector = trust.h2()?.configure()?;
    let listener = TcpListener::bind("[::1]:0").await?;
    let address = listener.local_addr()?;
    let server = async {
        let (socket, _peer) = listener.accept().await?;
        let tls = tokio_boring::accept(&acceptor, socket).await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(KeyExchange::from(tls.ssl()))
    };
    let client = async {
        let socket = TcpStream::connect(address).await?;
        let tls = tokio_boring::connect(connector, "localhost", socket).await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(KeyExchange::from(tls.ssl()))
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::try_join!(client, server)
    })
    .await?
}

fn peer(directory: &Path, group: &str, version: SslVersion) -> TestResult<SslAcceptor> {
    let identity = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])?;
    let certificate = directory.join("certificate.pem");
    let key = directory.join("key.pem");
    std::fs::write(&certificate, identity.cert.pem())?;
    let private = zeroize::Zeroizing::new(identity.signing_key.serialize_pem());
    std::fs::write(&key, private.as_bytes())?;
    let mut context = SslAcceptor::mozilla_intermediate_v5(SslMethod::tls())?;
    context.set_min_proto_version(Some(version))?;
    context.set_max_proto_version(Some(version))?;
    context.set_curves_list(group)?;
    context.set_certificate_chain_file(certificate)?;
    context.set_private_key_file(key, SslFiletype::PEM)?;
    context.check_private_key()?;
    Ok(context.build())
}

#[test]
fn every_group_keeps_its_stable_label_and_census_value() {
    for (group, label) in [
        (KeyExchange::X25519MlKem768, "x25519_mlkem768"),
        (KeyExchange::X25519, "x25519"),
        (KeyExchange::P256, "p256"),
        (KeyExchange::P384, "p384"),
        (KeyExchange::Other, "other"),
    ] {
        assert_eq!(group.as_str(), label);
        assert_eq!(StreamKeyExchange::from(group).as_str(), label);
    }
}

#[test]
fn only_the_hybrid_group_passes_and_a_refusal_is_a_tls_failure() -> TestResult {
    assert_eq!(
        KeyExchange::X25519MlKem768.check()?,
        KeyExchange::X25519MlKem768
    );
    for group in [
        KeyExchange::X25519,
        KeyExchange::P256,
        KeyExchange::P384,
        KeyExchange::Other,
    ] {
        let Err(refused) = group.require(Transport::H2, StreamPeer::Client) else {
            return Err(format!("{group:?} passed the hybrid requirement").into());
        };
        assert_eq!(refused.reason(), Reason::Tls);
    }
    Ok(())
}

#[test]
fn trust_reports_only_the_roots_its_caller_chose() {
    assert_eq!(Trust::default().roots(), None);
    assert_eq!(
        Trust::with_roots("/fixture/roots.pem").roots(),
        Some(Path::new("/fixture/roots.pem"))
    );
}
