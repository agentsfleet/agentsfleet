//! The correlation token every response carries.
//!
//! `req_` followed by twelve lowercase hex characters —
//! `handlers/common.zig`'s `requestId`, byte-for-byte. The shape is not an
//! internal detail: it is short enough for a person to read one off a browser
//! screenshot and type it into a support ticket, which is most of what it is
//! for. Lengthening it, or swapping it for a UUID, would break that use before
//! it broke any code.
//!
//! # Why an id is minted per response, not stamped per request
//!
//! Because that is where the daemon's ids come from, and pretending otherwise
//! was a real bug in this module's first draft: it read the id out of a
//! [`tower_http`]-stamped request extension, so an id was present only if a
//! layer nobody had written yet was mounted, and its absence degraded silently
//! to the sentinel. That made [`UNKNOWN_REQUEST_ID`] mean three different
//! things — nothing stamped, non-ASCII stamped, or the layer simply missing —
//! and coupled the refusal path to a foreign crate's newtype over a value this
//! product defines.
//!
//! Minting here restores the sentinel's single meaning: entropy failed. If a
//! request-scoped id is later wanted — echoed in a response header, threaded
//! through a span — that is a deliberate change with one minter, and this is
//! the minter.
//!
//! [`tower_http`]: https://docs.rs/tower-http

use std::fmt::{self, Display, Formatter};

use afd_core::error_code;
use afd_crypto::entropy::Entropy;

/// The prefix that makes an id recognisable in a log line or a ticket.
const PREFIX: &str = "req_";

/// Random bytes behind one id: six, which is exactly the twelve hex characters
/// `requestId` keeps after truncating its own sixteen.
const ENTROPY_BYTES: usize = 6;

/// The id used when entropy is unavailable.
///
/// `handlers/common.zig`'s `UNKNOWN_REQUEST_ID`. A response is already being
/// written when this is reached and an envelope without a `request_id` would be
/// a shape change, so the field says plainly that there is no id rather than
/// carrying an invented one that support would then fail to find.
pub const UNKNOWN_REQUEST_ID: &str = "req_unknown";

/// One request's correlation token.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RequestId(String);

impl RequestId {
    /// A fresh id, drawing from the operating system.
    #[must_use]
    pub fn mint() -> Self {
        Self::mint_from(&Entropy::new())
    }

    /// A fresh id, drawing from `entropy`.
    ///
    /// Infallible on purpose. The caller is mid-response — there is nothing to
    /// return an error TO — so an entropy failure degrades to
    /// [`UNKNOWN_REQUEST_ID`], which is what `requestId` does with its own two
    /// failure paths.
    #[must_use]
    pub fn mint_from(entropy: &Entropy) -> Self {
        let mut bytes = [0u8; ENTROPY_BYTES];
        if entropy.fill(&mut bytes).is_err() {
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            // `error`, where the Zig minter logs nothing at all. A host that
            // cannot produce six random bytes cannot seal a secret either, so
            // the unnamed request this degrades to is the least of what is
            // about to go wrong — and it is the first symptom that surfaces.
            tracing::error!(
                error_code = code,
                event = "request_id_entropy_unavailable",
                "entropy unavailable — this request is unidentified, and \
                 anything that needs a key is about to fail"
            );
            return Self(UNKNOWN_REQUEST_ID.to_owned());
        }
        // Folded big-endian into the low 48 bits of a `u64`, so one `{:012x}`
        // renders the whole id. Six bytes cannot overflow a `u64`, and a fold
        // says that structurally where indexing would only assert it.
        let packed = bytes
            .iter()
            .fold(0u64, |packed, &byte| (packed << 8) | u64::from(byte));
        Self(format!("{PREFIX}{packed:012x}"))
    }

    /// The id as it appears on the wire.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for RequestId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<RequestId> for String {
    fn from(id: RequestId) -> Self {
        id.0
    }
}
