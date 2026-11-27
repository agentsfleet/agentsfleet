//! Signature verification, and the signers suites make signatures with.
//!
//! A toolbox release signs its manifest the way `cosign sign-blob` does with a
//! key pair: ECDSA over P-256 and SHA-256, the signature ASN.1 DER. The runner
//! checks it offline against the key it is built with, before it reads one
//! field of the manifest.

use aws_lc_rs::signature::{ECDSA_P256_SHA256_ASN1, UnparsedPublicKey};

use crate::error::{Error, ErrorKind, Result};

/// Refuses unless `signature`, an ASN.1 DER ECDSA signature over P-256 and
/// SHA-256, is `public_key`'s over `message`.
///
/// `public_key` is the DER `SubjectPublicKeyInfo` a PEM `PUBLIC KEY` holds.
///
/// # Errors
/// A signature-mismatch error when the key is not a P-256 key, the signature is
/// not DER, or the signature is not the key's over `message`. One answer for
/// all three: a caller acts on none of them differently.
pub fn verify_ecdsa_p256_sha256(public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, public_key)
        .verify(message, signature)
        .map_err(|_unverified| Error::from(ErrorKind::SignatureMismatch))
}

/// Signers for suites: a release key made per test, and the fixed RSA key a
/// token verifier's suite signs with. Behind `test-util`, so a release build
/// signs nothing.
#[cfg(feature = "test-util")]
pub mod testing {
    #![expect(
        clippy::expect_used,
        reason = "test support: a key that cannot be made or used is a broken suite, \
                  and failing loudly at the fixture is the correct outcome"
    )]

    use aws_lc_rs::encoding::AsDer as _;
    use aws_lc_rs::rand::SystemRandom;
    use aws_lc_rs::signature::{
        ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair as _, RSA_PKCS1_SHA256, RsaKeyPair,
        RsaPublicKeyComponents,
    };

    /// A fresh ECDSA P-256 key, signing as `cosign sign-blob` does.
    #[derive(Debug)]
    pub struct EcdsaP256Signer(EcdsaKeyPair);

    impl EcdsaP256Signer {
        /// A key no other suite holds.
        ///
        /// # Panics
        /// When the system entropy source fails.
        #[must_use]
        pub fn generate() -> Self {
            Self(
                EcdsaKeyPair::generate(&ECDSA_P256_SHA256_ASN1_SIGNING)
                    .expect("a P-256 key from the system entropy source"),
            )
        }

        /// The public half as a DER `SubjectPublicKeyInfo`, the form
        /// [`super::verify_ecdsa_p256_sha256`] takes.
        ///
        /// # Panics
        /// When AWS-LC cannot encode a key it generated.
        #[must_use]
        pub fn public_key_der(&self) -> Vec<u8> {
            self.0
                .public_key()
                .as_der()
                .expect("the public half of a generated key encodes")
                .as_ref()
                .to_vec()
        }

        /// `message`'s signature, ASN.1 DER.
        ///
        /// # Panics
        /// When the system entropy source fails.
        #[must_use]
        pub fn sign(&self, message: &[u8]) -> Vec<u8> {
            self.0
                .sign(&SystemRandom::new(), message)
                .expect("a signature from the system entropy source")
                .as_ref()
                .to_vec()
        }
    }

    /// An RSA key signing RS256 (PKCS#1 v1.5 over SHA-256), as a JSON Web
    /// Token issuer does.
    #[derive(Debug)]
    pub struct Rs256Signer(RsaKeyPair);

    impl Rs256Signer {
        /// The key a PKCS#1 `RSAPrivateKey` DER holds.
        ///
        /// # Panics
        /// When `der` is not an RSA private key AWS-LC accepts.
        #[must_use]
        pub fn from_pkcs1_der(der: &[u8]) -> Self {
            Self(RsaKeyPair::from_der(der).expect("a PKCS#1 RSA private key"))
        }

        /// The public modulus and exponent, big-endian, as a JSON Web Key's
        /// `n` and `e` carry them.
        #[must_use]
        pub fn public_components(&self) -> (Vec<u8>, Vec<u8>) {
            let components: RsaPublicKeyComponents<Vec<u8>> = self.0.public_key().into();
            (components.n, components.e)
        }

        /// `message`'s RS256 signature, as wide as the modulus.
        ///
        /// # Panics
        /// When AWS-LC refuses to sign with the key.
        #[must_use]
        pub fn sign(&self, message: &[u8]) -> Vec<u8> {
            let mut signature = vec![0_u8; self.0.public_key().modulus_len()];
            self.0
                .sign(
                    &RSA_PKCS1_SHA256,
                    &SystemRandom::new(),
                    message,
                    &mut signature,
                )
                .expect("an RS256 signature");
            signature
        }
    }
}
