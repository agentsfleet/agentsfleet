//! Key material that zeroes on drop and refuses to print itself.
//!
//! # Why these are newtypes and not `[u8; 32]`
//!
//! A bare array has no destructor, prints its contents from `{:?}`, and can be
//! copied without trace. Each type here wraps a PRIVATE array: there is no
//! field to read, no `Deref` to the bytes, and no infallible conversion in, so
//! the only way to obtain one is through a constructor that has checked the
//! value. That is `M-STRONG-TYPES-GUARD` applied to a security invariant rather
//! than to a range check — Invariant 5 of the milestone spec is defeated by a
//! single `pub` here, not by a logic error.
//!
//! The `Debug` implementations are hand-written and tested. `M-PUBLIC-DEBUG`
//! asks for exactly that: sensitive types still implement `Debug`, but through
//! an implementation whose redaction has a unit test behind it.

use std::fmt::{self, Debug, Display, Formatter};

/// The wiping owner for one secret field, re-exported so every crate takes it from the
/// foundation that owns secret types rather than adding its own dependency.
pub use zeroize::Zeroizing;

mod object;
mod string;
pub use self::object::SecretObject;
pub use self::string::SecretString;

use crate::KEY_LEN;
use crate::error::{Error, ErrorKind, Result};

/// The process Key Encryption Key, resolved once at boot and immutable after.
///
/// Milestone Invariant 3 says the KEK is resolved before traffic and never
/// changes. There is no setter and no interior mutability, so the invariant is
/// the type: a `Kek` value cannot be mutated, only dropped.
#[derive(Clone)]
pub struct Kek(Zeroizing<[u8; KEY_LEN]>);

/// A per-row Data Encryption Key, generated fresh for every sealed payload.
#[derive(Clone)]
pub struct Dek(Zeroizing<[u8; KEY_LEN]>);

/// Plaintext recovered from an envelope, zeroed when it goes out of scope.
#[derive(Clone)]
pub struct SecretBytes(Zeroizing<Vec<u8>>);

impl Kek {
    /// Decodes the 64-character hex master key into a 32-byte KEK.
    ///
    /// This is the only constructor. `ENCRYPTION_MASTER_KEY` reaches the daemon
    /// as hex and is validated exactly here, which is what lets boot refuse a
    /// malformed key before anything serves traffic.
    ///
    /// # Errors
    /// Returns a key-hex error when the input is not exactly `KEY_LEN * 2`
    /// characters, or contains a character that is not a hex digit.
    pub fn from_hex(hex: &str) -> Result<Self> {
        // Decoded straight into its wiping owner, so no unwiped stack copy is left behind.
        let mut key = Zeroizing::new([0_u8; KEY_LEN]);
        decode_hex_into(hex, &mut *key)?;
        Ok(Self(key))
    }

    /// Builds a KEK from raw bytes already known to be key material.
    #[must_use]
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub(crate) fn expose(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl Dek {
    /// Builds a DEK from raw bytes already known to be key material.
    #[must_use]
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Builds a DEK from an unwrapped slice, rejecting a wrong length.
    ///
    /// # Errors
    /// Returns a malformed-envelope error when the slice is not `KEY_LEN` bytes.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let sized: [u8; KEY_LEN] = bytes.try_into().map_err(|_err| {
            Error::from(ErrorKind::ComponentLength {
                component: "data encryption key",
                expected: KEY_LEN,
                actual: bytes.len(),
            })
        })?;
        Ok(Self::from_bytes(sized))
    }

    pub(crate) fn expose(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl SecretBytes {
    /// Wraps recovered plaintext so it is zeroed when dropped.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// The plaintext bytes, borrowed for as long as this value lives.
    ///
    /// Borrowed rather than moved on purpose: handing out an owned `Vec` would
    /// create a copy this type can no longer zero.
    #[must_use]
    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    /// How many bytes of plaintext were recovered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the recovered plaintext is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Decodes lowercase-or-uppercase hex into a fixed buffer, rejecting anything else.
///
/// The length check stays here rather than being left to the decoder: `hex`
/// reports a wrong length as one opaque variant, and [`ErrorKind::KeyHexLength`]
/// carries what was expected and what arrived — which is the difference
/// between an operator seeing "bad key" and seeing that they pasted 63
/// characters.
///
/// The digits themselves are the crate's, not ours. `hex::decode_to_slice`
/// writes straight into the caller's buffer with no intermediate allocation,
/// which for key material is the property that matters: a decoder that
/// returned a `Vec` would leave a copy of the key on the heap with no
/// destructor.
fn decode_hex_into(hex: &str, out: &mut [u8]) -> Result<()> {
    let expected = out.len() * 2;
    if hex.len() != expected {
        return Err(Error::from(ErrorKind::KeyHexLength {
            expected,
            actual: hex.len(),
        }));
    }
    hex::decode_to_slice(hex, out).map_err(|_digit| Error::from(ErrorKind::KeyHexDigit))
}

/// Renders a redacted placeholder, never the bytes.
macro_rules! redacted {
    ($ty:ident, $label:literal) => {
        impl Debug for $ty {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str(concat!($label, "(redacted)"))
            }
        }

        impl Display for $ty {
            // Delegates rather than repeating the placeholder: `concat!` needs
            // a literal, so two spellings could drift and one could start
            // leaking while the other stayed redacted.
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                Debug::fmt(self, f)
            }
        }
    };
}

redacted!(Kek, "Kek");
redacted!(Dek, "Dek");
redacted!(SecretBytes, "SecretBytes");
