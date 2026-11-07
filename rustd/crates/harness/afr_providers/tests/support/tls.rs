//! Ephemeral CA and pinned test DNS; production keeps its guarded resolver.

use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;

pub(super) const HOST: &str = "fake.provider.test";

pub(super) fn server() -> (reqwest::Certificate, TlsAcceptor) {
    let mut authority = CertificateParams::new(Vec::<String>::new()).unwrap();
    authority.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    authority.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let issuer = CertifiedIssuer::self_signed(authority, KeyPair::generate().unwrap()).unwrap();
    let leaf_key = KeyPair::generate().unwrap();
    let leaf = CertificateParams::new(vec![HOST.to_owned()])
        .unwrap()
        .signed_by(&leaf_key, &issuer)
        .unwrap();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![leaf.der().clone()], key)
    .unwrap();
    let root = reqwest::Certificate::from_der(issuer.der()).unwrap();
    (root, TlsAcceptor::from(Arc::new(config)))
}

pub(super) fn client(root: reqwest::Certificate, address: SocketAddr) -> reqwest::Client {
    #[expect(
        clippy::disallowed_methods,
        reason = "real TLS fixture pins only its generated hostname to its listener; production uses the guarded resolver"
    )]
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .add_root_certificate(root)
        .resolve(HOST, address)
        .build()
        .unwrap()
}
