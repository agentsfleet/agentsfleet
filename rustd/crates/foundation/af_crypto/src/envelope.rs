//! The two-layer envelope the vault stores, and the sealer that produces it.
//!
//! # Layout
//!
//! Six components, which are the six ciphertext columns of `vault.secrets` in
//! the order an open reads them:
//!
//! | Component | Bytes | Produced by |
//! |---|---|---|
//! | wrapped DEK | 32 | AES-256-GCM over the DEK, under the KEK |
//! | DEK nonce | 12 | that operation's nonce |
//! | DEK tag | 16 | that operation's tag |
//! | payload nonce | 12 | the payload operation's nonce |
//! | payload ciphertext | variable | AES-256-GCM over the plaintext, under the DEK |
//! | payload tag | 16 | the payload operation's tag |
//!
//! Both operations bind the same [`Aad`], so the wrap and the payload cannot be
//! separated and recombined across rows.
//!
//! # Sealing needs a sealer, opening does not
//!
//! [`Envelope::open`] is pure: given the ciphertext and the KEK it either
//! recovers the plaintext or it does not. [`Sealer::seal`] consumes entropy, so
//! it hangs off a value that owns an entropy source rather than being a free
//! function — which is what makes the nonce mockable at all.

use aws_lc_rs::aead::{AES_256_GCM, Aad as AeadAad, LessSafeKey, Nonce, UnboundKey};
use zeroize::Zeroizing;

use crate::aad::Aad;
use crate::entropy::Source;
use crate::error::{Error, ErrorKind, Result};
use crate::secret::{Dek, Kek, SecretBytes};
use crate::{KEY_LEN, NONCE_LEN, TAG_LEN};

/// One stored credential's ciphertext, as six components plus its KEK version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    wrapped_dek: Vec<u8>,
    dek_nonce: [u8; NONCE_LEN],
    dek_tag: [u8; TAG_LEN],
    payload_nonce: [u8; NONCE_LEN],
    payload_ciphertext: Vec<u8>,
    payload_tag: [u8; TAG_LEN],
    kek_version: i32,
}

/// One AES-256-GCM result: ciphertext, the nonce used, and the detached tag.
///
/// Named rather than returned as a bare triple so the seal path reads as three
/// things with meanings instead of `.0`, `.1` and `.2`.
struct Encrypted {
    ciphertext: Vec<u8>,
    nonce: [u8; NONCE_LEN],
    tag: [u8; TAG_LEN],
}

/// Produces envelopes, owning the entropy the nonces come from.
#[derive(Debug, Clone)]
pub struct Sealer {
    entropy: Source,
}

impl Sealer {
    /// Builds a sealer drawing keys and nonces from AWS-LC's generator.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entropy: Source::Native,
        }
    }

    /// Builds a sealer whose nonces come from a controller the caller drives.
    ///
    /// Returns the pair rather than accepting a controller, per
    /// `M-MOCKABLE-SYSCALLS`: two sealers sharing one controller would make the
    /// nonce sequence ambiguous.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn new_mocked() -> (Self, crate::entropy::MockCtrl) {
        let ctrl = crate::entropy::MockCtrl::new();
        (
            Self {
                entropy: Source::Mocked(ctrl.clone()),
            },
            ctrl,
        )
    }

    /// Seals `plaintext` under a fresh DEK, itself wrapped under `kek`.
    ///
    /// # Errors
    /// Returns an entropy error when the nonce or key source fails, and an
    /// open-failed error if the AEAD refuses to encrypt.
    pub fn seal(&self, kek: &Kek, aad: &Aad, plaintext: &[u8]) -> Result<Envelope> {
        let mut dek_bytes = Zeroizing::new([0_u8; KEY_LEN]);
        self.entropy.fill(dek_bytes.as_mut_slice())?;
        let dek = Dek::from_slice(dek_bytes.as_slice())?;

        let wrapped = self.encrypt(kek.expose(), aad, dek.expose().as_slice())?;
        let payload = self.encrypt(dek.expose(), aad, plaintext)?;

        Ok(Envelope {
            wrapped_dek: wrapped.ciphertext,
            dek_nonce: wrapped.nonce,
            dek_tag: wrapped.tag,
            payload_nonce: payload.nonce,
            payload_ciphertext: payload.ciphertext,
            payload_tag: payload.tag,
            kek_version: crate::KEK_VERSION,
        })
    }

    /// One AES-256-GCM operation, returning ciphertext, nonce and tag apart.
    ///
    /// Detached rather than appended: `vault.secrets` stores the tag in its own
    /// column, so a combined ciphertext-plus-tag buffer would have to be split
    /// again on the way out and would invite an off-by-one at the seam.
    fn encrypt(&self, key: &[u8; KEY_LEN], aad: &Aad, plaintext: &[u8]) -> Result<Encrypted> {
        let mut nonce_bytes = [0_u8; NONCE_LEN];
        self.entropy.fill(&mut nonce_bytes)?;

        // Wiped if the seal is refused: when it wraps a DEK, this copy is the key.
        let mut buffer = Zeroizing::new(plaintext.to_vec());
        // `Unspecified` is a unit struct by design — an AEAD deliberately says
        // only that it refused, never which check failed — so there is no cause
        // to preserve here, and `_err` records that rather than hiding one.
        let tag = cipher(key)?
            .seal_in_place_separate_tag(
                Nonce::assume_unique_for_key(nonce_bytes),
                AeadAad::from(aad.as_bytes()),
                buffer.as_mut_slice(),
            )
            .map_err(|_err| Error::from(ErrorKind::OpenFailed))?;

        Ok(Encrypted {
            ciphertext: std::mem::take(&mut *buffer),
            nonce: nonce_bytes,
            // `AES_256_GCM` always writes `TAG_LEN` bytes; aws-lc-rs hands the
            // tag over as a slice, so the width is checked rather than assumed.
            tag: fixed(tag.as_ref(), "tag")?,
        })
    }
}

impl Default for Sealer {
    fn default() -> Self {
        Self::new()
    }
}

impl Envelope {
    /// Rebuilds an envelope from the six stored columns and its version.
    ///
    /// Fallible because the lengths are invariants this type guarantees to
    /// every later use (`M-STRONG-TYPES-GUARD`): a short nonce read out of a
    /// damaged row is rejected here rather than panicking inside the AEAD.
    ///
    /// # Errors
    /// Returns a malformed-envelope error when any fixed-width component —
    /// the wrapped DEK included — has the wrong length, or when `kek_version`
    /// is not the supported version.
    pub fn from_parts(
        wrapped_dek: Vec<u8>,
        dek_nonce: &[u8],
        dek_tag: &[u8],
        payload_nonce: &[u8],
        payload_ciphertext: Vec<u8>,
        payload_tag: &[u8],
        kek_version: i32,
    ) -> Result<Self> {
        if kek_version != crate::KEK_VERSION {
            return Err(Error::from(ErrorKind::UnsupportedVersion {
                found: kek_version,
                supported: crate::KEK_VERSION,
            }));
        }
        // The wrapped DEK is fixed-width like the nonces and tags: a detached
        // AES-GCM ciphertext is as long as its plaintext, and that plaintext is
        // always a `KEY_LEN` key. Checking it here keeps the column honest at
        // the boundary instead of surfacing as an unopenable envelope later.
        if wrapped_dek.len() != KEY_LEN {
            return Err(Error::from(ErrorKind::ComponentLength {
                component: "wrapped dek",
                expected: KEY_LEN,
                actual: wrapped_dek.len(),
            }));
        }
        Ok(Self {
            wrapped_dek,
            dek_nonce: fixed(dek_nonce, "dek nonce")?,
            dek_tag: fixed(dek_tag, "dek tag")?,
            payload_nonce: fixed(payload_nonce, "payload nonce")?,
            payload_ciphertext,
            payload_tag: fixed(payload_tag, "payload tag")?,
            kek_version,
        })
    }

    /// Unwraps the DEK under `kek`, then recovers the payload under that DEK.
    ///
    /// # Errors
    /// Returns an open-failed error when either layer does not authenticate.
    /// The two are deliberately indistinguishable — see [`crate::error`].
    pub fn open(&self, kek: &Kek, aad: &Aad) -> Result<SecretBytes> {
        let dek_plain = decrypt(
            kek.expose(),
            aad,
            &self.dek_nonce,
            &self.wrapped_dek,
            &self.dek_tag,
        )?;
        let dek = Dek::from_slice(dek_plain.expose())?;

        decrypt(
            dek.expose(),
            aad,
            &self.payload_nonce,
            &self.payload_ciphertext,
            &self.payload_tag,
        )
    }

    /// The wrapped Data Encryption Key, as stored.
    #[must_use]
    pub fn wrapped_dek(&self) -> &[u8] {
        &self.wrapped_dek
    }

    /// The nonce the DEK was wrapped under.
    #[must_use]
    pub const fn dek_nonce(&self) -> &[u8; NONCE_LEN] {
        &self.dek_nonce
    }

    /// The authentication tag over the wrapped DEK.
    #[must_use]
    pub const fn dek_tag(&self) -> &[u8; TAG_LEN] {
        &self.dek_tag
    }

    /// The nonce the payload was encrypted under.
    #[must_use]
    pub const fn payload_nonce(&self) -> &[u8; NONCE_LEN] {
        &self.payload_nonce
    }

    /// The encrypted payload, as stored.
    #[must_use]
    pub fn payload_ciphertext(&self) -> &[u8] {
        &self.payload_ciphertext
    }

    /// The authentication tag over the payload.
    #[must_use]
    pub const fn payload_tag(&self) -> &[u8; TAG_LEN] {
        &self.payload_tag
    }

    /// The KEK version this envelope was written under.
    #[must_use]
    pub const fn kek_version(&self) -> i32 {
        self.kek_version
    }
}

/// One AES-256-GCM open, with the tag supplied detached.
fn decrypt(
    key: &[u8; KEY_LEN],
    aad: &Aad,
    nonce: &[u8; NONCE_LEN],
    ciphertext: &[u8],
    tag: &[u8; TAG_LEN],
) -> Result<SecretBytes> {
    // Owned by its wiper before the open runs: a refused open leaves the
    // buffer overwritten in an unspecified way, and that is wiped on drop too.
    let mut buffer = Zeroizing::new(ciphertext.to_vec());
    // Same reasoning as the seal side: `Unspecified` carries nothing, and
    // collapsing a tag failure and a wrong key into one outcome is the point.
    cipher(key)?
        .open_in_place_separate_tag(
            Nonce::assume_unique_for_key(*nonce),
            AeadAad::from(aad.as_bytes()),
            tag,
            buffer.as_mut_slice(),
        )
        .map_err(|_err| Error::from(ErrorKind::OpenFailed))?;
    Ok(SecretBytes::new(std::mem::take(&mut *buffer)))
}

/// The AES-256-GCM key for one operation.
///
/// AWS-LC holds the expanded key in its own allocation and cleanses it when
/// the key drops (`EVP_AEAD_CTX_free` frees through `OPENSSL_free`, which
/// zeroes), so no copy outlives the operation; the raw bytes stay in their
/// `Zeroizing` owner.
pub(crate) fn cipher(key: &[u8; KEY_LEN]) -> Result<LessSafeKey> {
    // A 32-byte key is the one length AES-256 accepts; the refusal is AWS-LC's
    // only other failure, an allocation it could not make.
    let unbound =
        UnboundKey::new(&AES_256_GCM, key).map_err(|_err| Error::from(ErrorKind::OpenFailed))?;
    Ok(LessSafeKey::new(unbound))
}

/// Narrows a stored slice to a fixed-width component, naming it on failure.
fn fixed<const N: usize>(bytes: &[u8], component: &'static str) -> Result<[u8; N]> {
    bytes.try_into().map_err(|_err| {
        Error::from(ErrorKind::ComponentLength {
            component,
            expected: N,
            actual: bytes.len(),
        })
    })
}
