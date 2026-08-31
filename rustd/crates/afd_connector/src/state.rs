//! The signed, single-use state that binds a connect round-trip to the
//! workspace AND the person who started it.
//!
//! # What the provider sees, and what it does not
//!
//! The state rides the provider's URL, so it is public by construction. What it
//! carries is a workspace id, a KEYED TAG of the starter's subject, a nonce and
//! an expiry — never the raw identity-provider subject, which is why the tag is
//! an HMAC rather than the value. What makes it trustworthy is three
//! independent properties:
//!
//! * **Unforgeable** — HMAC-SHA256 over the payload under this deployment's
//!   signing secret, domain-separated by the provider's own prefix, so one
//!   connector's state cannot cross-verify as another's.
//! * **Time-bounded** — an embedded expiry the completion refuses past.
//! * **Single-use** — a nonce this daemon remembers, deleted on first use. That
//!   half lives in [`nonce`], because it is the only part that needs a store.
//!
//! # Wire shape, which is a data format rather than a spelling
//!
//! ```text
//!   base64url(workspace "|" subject_tag "|" nonce "|" exp_ms) "." hex(mac)
//! ```
//!
//! Inherited from `state.zig` because the format is sound, NOT because a
//! cutover depends on it — nothing is in production, so there are no in-flight
//! connects to preserve. A wire format on this port stands on its own merits.
//!
//! And a caution for whoever reads the HMAC and infers more from it than is
//! there: the signature is not what makes this safe. An opaque random token
//! with the workspace, subject and expiry held in the store instead of in the
//! token reaches the same unforgeability, the same starter binding and the same
//! single-use — `oauth2::CsrfToken` is exactly that, and it is not weaker. What
//! the signature buys is that the store answers ONE question ("spent?") rather
//! than being the authority on what the token means. See the spec's Discovery
//! log, where the comparison is recorded with its measurements.
//!
//! # The verify does not consume, and that ordering is load-bearing
//!
//! [`verify`] answers whether the state is genuine and unexpired, and stops
//! there. The caller then compares the returning person against
//! [`Verified::subject_matches`] and re-authorises the workspace BEFORE calling
//! [`nonce::consume`]. Consuming first would let any authenticated person burn
//! somebody else's in-flight connect by replaying its URL.

pub mod nonce;

#[cfg(test)]
mod tests;

use afd_core::clock::UnixMillis;
use afd_crypto::mac::HmacSha256Tag;
use afd_crypto::secret::SecretBytes;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64URL;
use subtle::ConstantTimeEq as _;

use crate::registry::{STATE_TTL_SECONDS, StateBinding};

/// What separates the payload's four fields.
const FIELD_SEP: char = '|';

/// What separates the payload from its authentication tag.
const MAC_SEP: char = '.';

/// What the subject tag's HMAC is domain-separated by, beneath the provider's.
///
/// A second prefix rather than none, so a tag can never equal the state MAC
/// over the same bytes: the two are computed under one secret and would
/// otherwise be one construction serving two purposes.
const SUBJECT_TAG_PREFIX: &str = "subject:v1:";

/// Milliseconds in a second, for the expiry arithmetic.
const MS_PER_SECOND: i64 = 1_000;

/// A state that was minted, and the nonce that makes it single-use.
///
/// Both, because they are written to two different places: the state goes into
/// the provider's URL and the nonce into this daemon's own store. A mint that
/// answered only the state would leave the caller re-deriving the nonce by
/// parsing what it just built.
#[derive(Debug, Clone)]
pub struct Minted {
    /// What rides the provider's `state` query parameter.
    pub state: String,
    /// What the single-use slot is remembered under.
    pub nonce: String,
}

/// Why a presented state is not one this daemon will act on.
///
/// Distinguished for the LOG and collapsed for the answer: an operator reading
/// why a connect failed needs these apart, and a caller replaying states must
/// not learn which check they got past. The caller answers one code for all of
/// them — `callback.zig` answers one `UZ-CONN-002` for the same reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    /// Not the shape a state has: no tag separator, or not four fields.
    Malformed,
    /// The tag did not match the one this deployment computes.
    ///
    /// A forgery, a state from another connector's domain, or one signed under
    /// a secret this deployment has since rotated.
    BadSignature,
    /// Genuine, and past the moment it stopped being usable.
    Expired,
    /// Genuine, unexpired, and presented by somebody who did not start it.
    ///
    /// Its own variant rather than folded into [`Self::BadSignature`], because
    /// the two send an operator to different places: a forged state is somebody
    /// probing, and this is an authenticated person completing a round-trip
    /// that was not theirs — which is what the subject binding exists to stop
    /// and what its log line has to be able to say.
    ForeignSubject,
}

impl Rejected {
    /// The word a log line names this rejection by.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Malformed => "state_malformed",
            Self::BadSignature => "state_bad_signature",
            Self::Expired => "state_expired",
            Self::ForeignSubject => "state_foreign_subject",
        }
    }
}

/// A state this deployment signed, still inside its window.
///
/// Says nothing yet about WHO is completing the round-trip: that is
/// [`Verified::subject_matches`], asked by the caller against the person it
/// authenticated, and it is a separate step because a genuine state presented
/// by the wrong person is a different refusal from a forged one.
#[derive(Debug, Clone)]
pub struct Verified {
    /// The workspace the connect was started for.
    workspace: String,
    /// The keyed tag of the person who started it.
    subject_tag: String,
    /// What the single-use slot is remembered under.
    nonce: String,
}

impl Verified {
    /// The workspace this connect was started for.
    #[must_use]
    pub fn workspace(&self) -> &str {
        &self.workspace
    }

    /// What the single-use slot is remembered under.
    #[must_use]
    pub fn nonce(&self) -> &str {
        &self.nonce
    }

    /// Whether `subject` is the person who started this connect.
    ///
    /// Constant-time, over a tag rather than over the subject itself: the tag
    /// is keyed, so the comparison leaks nothing about the identity even to
    /// somebody holding the state (RULE CTM).
    #[must_use]
    pub fn subject_matches(
        &self,
        binding: StateBinding,
        secret: &SecretBytes,
        subject: &str,
    ) -> bool {
        let expected = subject_tag(binding, secret, subject);
        self.subject_tag
            .as_bytes()
            .ct_eq(expected.as_bytes())
            .into()
    }
}

/// Signs a state binding `workspace` and `subject`, valid for the flow's window.
///
/// `nonce` is supplied rather than drawn here: this half is pure, so a suite
/// can pin the exact bytes a state carries, and the entropy draw is the
/// caller's — see [`nonce::mint`].
#[must_use]
pub fn sign(
    binding: StateBinding,
    secret: &SecretBytes,
    workspace: &str,
    subject: &str,
    nonce: &str,
    now: UnixMillis,
) -> String {
    let expiry = now.saturating_add_millis(i64::from(STATE_TTL_SECONDS) * MS_PER_SECOND);
    let tag = subject_tag(binding, secret, subject);
    let payload = format!(
        "{workspace}{FIELD_SEP}{tag}{FIELD_SEP}{nonce}{FIELD_SEP}{}",
        expiry.as_millis(),
    );
    let mac = mac_hex(binding, secret, payload.as_bytes());
    format!("{}{MAC_SEP}{mac}", BASE64URL.encode(payload.as_bytes()))
}

/// Verifies a presented state's signature, shape and window.
///
/// Consumes nothing — see the module note on why the single-use step comes
/// after the caller has checked who is presenting it.
///
/// # Errors
/// [`Rejected`] for a state this daemon will not act on, with the reason for
/// the log and one answer for the caller.
pub fn verify(
    binding: StateBinding,
    secret: &SecretBytes,
    presented: &str,
    now: UnixMillis,
) -> Result<Verified, Rejected> {
    let (encoded, tag) = presented.rsplit_once(MAC_SEP).ok_or(Rejected::Malformed)?;
    let payload = BASE64URL
        .decode(encoded.as_bytes())
        .map_err(|_shape| Rejected::Malformed)?;

    // The tag is checked BEFORE the payload is split, so nothing downstream
    // ever reads fields out of bytes this deployment did not sign.
    let expected = mac_hex(binding, secret, &payload);
    let matched: bool = tag.as_bytes().ct_eq(expected.as_bytes()).into();
    if !matched {
        return Err(Rejected::BadSignature);
    }

    let payload = String::from_utf8(payload).map_err(|_text| Rejected::Malformed)?;
    // Destructured in one binding, trailing `None` included: a FIFTH field is a
    // payload this build does not understand and a signed one at that, which
    // means a newer daemon minted it. Refusing beats reading the four fields we
    // recognise and silently ignoring whatever the fifth was for.
    let mut fields = payload.split(FIELD_SEP);
    let (Some(workspace), Some(subject_tag), Some(nonce), Some(expiry), None) = (
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
    ) else {
        return Err(Rejected::Malformed);
    };

    let expiry: i64 = expiry.parse().map_err(|_digits| Rejected::Malformed)?;
    if now.as_millis() > expiry {
        return Err(Rejected::Expired);
    }

    Ok(Verified {
        workspace: workspace.to_owned(),
        subject_tag: subject_tag.to_owned(),
        nonce: nonce.to_owned(),
    })
}

/// The authentication tag over `payload`, in the provider's own domain.
fn mac_hex(binding: StateBinding, secret: &SecretBytes, payload: &[u8]) -> String {
    HmacSha256Tag::compute_peppered(secret, &[binding.domain_prefix.as_bytes(), payload]).to_hex()
}

/// The keyed tag standing in for a person's subject on a public URL.
fn subject_tag(binding: StateBinding, secret: &SecretBytes, subject: &str) -> String {
    HmacSha256Tag::compute_peppered(
        secret,
        &[
            binding.domain_prefix.as_bytes(),
            SUBJECT_TAG_PREFIX.as_bytes(),
            subject.as_bytes(),
        ],
    )
    .to_hex()
}
