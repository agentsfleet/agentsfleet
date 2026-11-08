//! An ephemeral certificate authority and pinned test DNS, so a fake provider
//! serves real TLS; production keeps its guarded resolver.

use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;

/// The name the fake provider's certificate is issued for.
pub const HOST: &str = "fake.provider.test";

/// A fresh authority's root, and an acceptor serving a leaf it signed for [`HOST`].
///
/// # Errors
/// A certificate or key cannot be generated, or the server configuration
/// refuses them; the failure keeps its source.
pub fn server() -> io::Result<(reqwest::Certificate, TlsAcceptor)> {
    let mut authority = CertificateParams::new(Vec::<String>::new()).map_err(io::Error::other)?;
    authority.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    authority.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let authority_key = KeyPair::generate().map_err(io::Error::other)?;
    let issuer =
        CertifiedIssuer::self_signed(authority, authority_key).map_err(io::Error::other)?;
    let leaf_key = KeyPair::generate().map_err(io::Error::other)?;
    let leaf = CertificateParams::new(vec![HOST.to_owned()])
        .map_err(io::Error::other)?
        .signed_by(&leaf_key, &issuer)
        .map_err(io::Error::other)?;
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(io::Error::other)?
    .with_no_client_auth()
    .with_single_cert(vec![leaf.der().clone()], key)
    .map_err(io::Error::other)?;
    let root = reqwest::Certificate::from_der(issuer.der()).map_err(io::Error::other)?;
    Ok((root, TlsAcceptor::from(Arc::new(config))))
}

/// A client trusting only `root`, resolving [`HOST`] to `address`.
///
/// # Errors
/// The client cannot be built; the failure keeps its source.
pub fn client(root: reqwest::Certificate, address: SocketAddr) -> io::Result<reqwest::Client> {
    reqwest::ClientBuilder::new()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .add_root_certificate(root)
        .resolve(HOST, address)
        .build()
        .map_err(io::Error::other)
}
