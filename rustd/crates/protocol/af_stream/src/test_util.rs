//! Runtime-generated identities for the local native-session boundary.

use crate::{
    Result,
    tls::{Identity, Trust},
};

/// Local issuer and matching leaf files, kept alive until all native owners join.
#[derive(Debug)]
pub struct IdentityFixture {
    directory: tempfile::TempDir,
}

impl IdentityFixture {
    /// Generate a local issuer and a leaf valid for the supplied server names.
    ///
    /// # Errors
    /// Preserves certificate generation and local filesystem failures.
    pub fn new(names: Vec<String>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let directory = tempfile::tempdir()?;
        let mut root = rcgen::CertificateParams::new(Vec::<String>::new())?;
        root.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        root.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign];
        let root = rcgen::CertifiedIssuer::self_signed(root, rcgen::KeyPair::generate()?)?;
        let key = rcgen::KeyPair::generate()?;
        let leaf = rcgen::CertificateParams::new(names)?.signed_by(&key, &root)?;
        std::fs::write(
            directory.path().join("chain.pem"),
            format!("{}{}", leaf.pem(), root.pem()),
        )?;
        std::fs::write(directory.path().join("roots.pem"), root.pem())?;
        let private = zeroize::Zeroizing::new(key.serialize_pem());
        let key_path = directory.path().join("key.pem");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        use std::io::Write as _;
        options.open(key_path)?.write_all(private.as_bytes())?;
        Ok(Self { directory })
    }

    /// Native server identity backed by this fixture's files.
    #[must_use]
    pub fn identity(&self) -> Identity {
        Identity::new(self.chain(), self.key())
    }

    /// Client trust for this fixture's private issuer.
    #[must_use]
    pub fn trust(&self) -> Trust {
        Trust::with_roots(self.root())
    }

    /// Chain delivered to the server.
    #[must_use]
    pub fn chain(&self) -> std::path::PathBuf {
        self.directory.path().join("chain.pem")
    }

    /// Protected server leaf key.
    #[must_use]
    pub fn key(&self) -> std::path::PathBuf {
        self.directory.path().join("key.pem")
    }

    /// Public root bundle delivered to clients.
    #[must_use]
    pub fn root(&self) -> std::path::PathBuf {
        self.directory.path().join("roots.pem")
    }
}
