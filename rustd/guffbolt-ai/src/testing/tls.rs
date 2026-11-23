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
    let (issuer, acceptor) = issue()?;
    let root = reqwest::Certificate::from_der(issuer.der()).map_err(io::Error::other)?;
    Ok((root, acceptor))
}

/// A fresh authority, and an acceptor serving a leaf it signed for [`HOST`].
fn issue() -> io::Result<(CertifiedIssuer<'static, KeyPair>, TlsAcceptor)> {
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
    Ok((issuer, TlsAcceptor::from(Arc::new(config))))
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

/// A separate fixture process trusts the same public issuer as its parent.
///
/// # Errors
/// Preserves file, certificate and client construction failures.
pub fn client_from_file(
    root: &std::path::Path,
    address: SocketAddr,
) -> io::Result<reqwest::Client> {
    let bytes = std::fs::read(root)?;
    let certificate = reqwest::Certificate::from_pem(&bytes).map_err(io::Error::other)?;
    client(certificate, address)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tokio::io::AsyncWriteExt as _;

    use super::*;

    /// A file in the system's scratch directory holding `contents`, named for
    /// this process and `name` so parallel suites do not share it.
    fn written(name: &str, contents: &str) -> io::Result<PathBuf> {
        let path =
            std::env::temp_dir().join(format!("guffbolt-ai-tls-{}-{name}.pem", std::process::id()));
        std::fs::write(&path, contents)?;
        Ok(path)
    }

    /// The status a client built from the root in `file` reads from a server
    /// whose leaf `acceptor` serves.
    async fn status_through(file: &std::path::Path, acceptor: TlsAcceptor) -> io::Result<u16> {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let served = tokio::spawn(async move {
            let (stream, _peer) = listener.accept().await?;
            let mut stream = acceptor.accept(stream).await?;
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n")
                .await?;
            stream.shutdown().await
        });
        let client = client_from_file(file, address)?;
        let answered = client
            .get(format!("https://{HOST}:{}/", address.port()))
            .send()
            .await
            .map_err(io::Error::other);
        served.abort();
        Ok(answered?.status().as_u16())
    }

    #[tokio::test]
    async fn a_client_from_the_issuers_file_reaches_the_leaf_it_signed() -> io::Result<()> {
        let (issuer, acceptor) = issue()?;
        let file = written("trusted", &issuer.pem())?;

        let status = status_through(&file, acceptor).await;

        std::fs::remove_file(&file)?;
        assert_eq!(status?, 204);
        Ok(())
    }

    #[tokio::test]
    async fn a_client_from_another_issuers_file_refuses_the_leaf() -> io::Result<()> {
        let (other, _unused) = issue()?;
        let (_issuer, acceptor) = issue()?;
        let file = written("untrusted", &other.pem())?;

        let status = status_through(&file, acceptor).await;

        std::fs::remove_file(&file)?;
        assert!(
            status.is_err_and(|failure| failure
                .get_ref()
                .is_some_and(<dyn std::error::Error + Send + Sync>::is::<reqwest::Error>)),
            "the client refuses a leaf its root did not sign"
        );
        Ok(())
    }

    #[test]
    fn a_missing_root_file_fails_as_not_found() {
        let missing = std::env::temp_dir().join("guffbolt-ai-tls-no-such-root.pem");

        let built = client_from_file(&missing, SocketAddr::from(([127, 0, 0, 1], 1)));

        assert!(built.is_err_and(|error| error.kind() == io::ErrorKind::NotFound));
    }

    #[test]
    fn a_root_file_that_is_no_certificate_fails_the_build() -> io::Result<()> {
        let file = written(
            "malformed",
            "-----BEGIN CERTIFICATE-----\n!!!!\n-----END CERTIFICATE-----\n",
        )?;

        let built = client_from_file(&file, SocketAddr::from(([127, 0, 0, 1], 1)));

        std::fs::remove_file(&file)?;
        assert!(built.is_err_and(|error| error.kind() == io::ErrorKind::Other));
        Ok(())
    }
}
