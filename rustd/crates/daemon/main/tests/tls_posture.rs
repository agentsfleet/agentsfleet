//! Every rustls connection this daemon opens offers the hybrid post-quantum
//! key exchange first (M244, Indy's TLS posture call).
//!
//! rustls is the TLS under sqlx, reqwest, async-nats, lettre and object_store,
//! so one handshake between a rustls client and server built the way those
//! libraries build theirs proves the workspace feature reached them all. The
//! handshake runs in memory: what is under test is the negotiation, not a
//! socket.

use std::sync::Arc;

use rustls::pki_types::{PrivateKeyDer, ServerName};
use rustls::{
    ClientConfig, ClientConnection, NamedGroup, RootCertStore, ServerConfig, ServerConnection,
};

type Fallible<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// A client and server over one self-signed certificate, on the provider
/// rustls installs by default.
fn pair() -> Fallible<(ClientConnection, ServerConnection)> {
    let issued = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])?;
    let certificate = issued.cert.der().clone();
    let key = PrivateKeyDer::Pkcs8(issued.signing_key.serialize_der().into());
    let server = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![certificate.clone()], key)?;
    let mut roots = RootCertStore::empty();
    roots.add(certificate)?;
    let client = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok((
        ClientConnection::new(Arc::new(client), ServerName::try_from("localhost")?)?,
        ServerConnection::new(Arc::new(server))?,
    ))
}

/// Run the handshake to completion through an in-memory buffer.
fn handshake(client: &mut ClientConnection, server: &mut ServerConnection) -> Fallible {
    while client.is_handshaking() || server.is_handshaking() {
        let mut flight = Vec::new();
        client.write_tls(&mut flight)?;
        server.read_tls(&mut flight.as_slice())?;
        server.process_new_packets()?;
        let mut reply = Vec::new();
        server.write_tls(&mut reply)?;
        client.read_tls(&mut reply.as_slice())?;
        client.process_new_packets()?;
    }
    Ok(())
}

#[test]
fn rustls_negotiates_the_hybrid_post_quantum_key_exchange() -> Fallible {
    let (mut client, mut server) = pair()?;
    handshake(&mut client, &mut server)?;
    let negotiated = client
        .negotiated_key_exchange_group()
        .map(|group| group.name());
    assert_eq!(
        negotiated,
        Some(NamedGroup::X25519MLKEM768),
        "the workspace's rustls offers X25519MLKEM768 first"
    );
    Ok(())
}
