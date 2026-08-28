//! What a mint attempt answers, for every connector.
//!
//! # Outcomes, not errors
//!
//! Only one of these is this daemon failing. A vendor that says the
//! installation is gone, and a fleet whose declaration the minted token did not
//! match, are both ANSWERS — the exchange worked and the result is that no
//! token may be handed over. Modelling them as `Err` would put them in the same
//! channel as a pool that would not answer, and the handler would have to take
//! them apart again to say anything useful.
//!
//! The one thing that never appears here is a token in a failure. A refusal
//! carries no credential, which is a property of the type rather than of the
//! care taken at each call site.

use zeroize::Zeroizing;

/// Whether a failed mint is worth trying again.
///
/// The distinction is the caller's backoff, not the wire: both classes answer
/// the same registry code, because a runner reacts to a mint failure the same
/// way regardless of whose fault it was. What it changes is whether this daemon
/// caches the failure and whether an operator is looking at their own
/// configuration or at a vendor's status page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retry {
    /// A vendor outage, a timeout, a 5xx. It may work in a minute.
    Transient,
    /// A malformed response, a rejected request, an unconfigured platform. It
    /// will not work in a minute.
    Permanent,
}

/// A credential this daemon is prepared to hand to a runner.
///
/// The token is [`Zeroizing`] so it is wiped when the last holder drops rather
/// than left in freed memory — it lives, briefly, in this process on its way to
/// exactly one response body (RULE VLT).
#[derive(Clone)]
pub struct Minted {
    /// The credential itself.
    pub token: Zeroizing<String>,
    /// When it stops working, in Unix milliseconds.
    pub expires_at_ms: i64,
    /// A REPLACEMENT refresh token the provider issued during the exchange, if
    /// it rotated one.
    ///
    /// `None` for every connector that does not rotate, and for a rotating one
    /// that returned the token it was posted. `Some` obliges the caller to
    /// write it back to the vault BEFORE the minted token is handed over:
    /// Zoho, Jira and Linear invalidate the posted refresh token the moment
    /// they issue a successor, so a handle left holding the old value is a
    /// connection that mints exactly once more — never — and reads as
    /// `reconnect_required` to a tenant who did nothing wrong.
    ///
    /// It rides here rather than being returned beside the outcome because it
    /// is produced by the one arm that also produces a token, and a second
    /// return value would let a caller drop it while keeping the credential.
    pub rotated_refresh_token: Option<Zeroizing<String>>,
}

// Hand-written: a derived `Debug` would print the token, and `Debug` is what a
// `tracing` field and a panic message both render through.
impl std::fmt::Debug for Minted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Minted")
            .field("expires_at_ms", &self.expires_at_ms)
            .field("rotated", &self.rotated_refresh_token.is_some())
            .finish_non_exhaustive()
    }
}

/// What one mint attempt produced.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// A usable credential.
    Ok(Minted),
    /// The tenant must connect the integration again.
    ///
    /// Reached when the vendor says the installation or grant is gone, and when
    /// the stored handle names none. Distinct from a failure because the remedy
    /// is a HUMAN's — no amount of retrying reconnects an App somebody removed.
    ReconnectRequired,
    /// The exchange did not produce a credential.
    MintFailed(Retry),
    /// This DEPLOYMENT holds no platform credential for the connector.
    ///
    /// Its own arm rather than a permanent failure, because it is the only
    /// outcome here that is nobody's fault but the operator's: the tenant
    /// connected an integration this daemon was never given an App or an OAuth
    /// client for, so no exchange was even attempted.
    Unconfigured,
    /// The handle names a connector this registry does not carry.
    ///
    /// Its own arm rather than a permanent failure: nothing went wrong, the
    /// integration was simply never connected for this workspace, and the
    /// sentence a caller reads should say so.
    UnknownIntegration,
}

impl Outcome {
    /// The credential, if this attempt produced one.
    #[must_use]
    pub const fn minted(&self) -> Option<&Minted> {
        match self {
            Self::Ok(minted) => Some(minted),
            Self::ReconnectRequired
            | Self::Unconfigured
            | Self::MintFailed(_)
            | Self::UnknownIntegration => None,
        }
    }
}
