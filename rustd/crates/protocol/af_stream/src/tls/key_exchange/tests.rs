//! Real TLS peers exercise preference, interoperability and the protocol minimum.

use std::{path::Path, time::Duration};

use boring::ssl::{SslAcceptor, SslFiletype, SslMethod, SslVersion};
use tokio::net::{TcpListener, TcpStream};

use super::KeyExchange;
use crate::tls::Trust;

type TestResult<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::test]
async fn tls13_classical_peers_remain_identified_when_they_cannot_offer_the_hybrid() -> TestResult {
    for (name, expected) in [
        ("X25519", KeyExchange::X25519),
        ("P-256", KeyExchange::P256),
        ("P-384", KeyExchange::P384),
    ] {
        let (client, server) = negotiate(name, SslVersion::TLS1_3).await?;
        assert_eq!(client, expected);
        assert_eq!(server, expected);
        assert_ne!(client.as_str(), KeyExchange::X25519MlKem768.as_str());
    }
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
    std::fs::write(&key, identity.signing_key.serialize_pem())?;
    let mut context = SslAcceptor::mozilla_intermediate_v5(SslMethod::tls())?;
    context.set_min_proto_version(Some(version))?;
    context.set_max_proto_version(Some(version))?;
    context.set_curves_list(group)?;
    context.set_certificate_chain_file(certificate)?;
    context.set_private_key_file(key, SslFiletype::PEM)?;
    context.check_private_key()?;
    Ok(context.build())
}
