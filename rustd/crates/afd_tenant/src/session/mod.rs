//! The device-flow login surface: relay only, no curve.
//!
//! # What this daemon does and does not do
//!
//! `docs/AUTH_DEVICE_LOGIN.md` puts the key exchange in the CLIENT — the
//! elliptic-curve work is `cli/src/lib/cli-flow.ts`'s, over P-256 through
//! `WebCrypto`. This daemon stores a public key it never uses, relays a
//! ciphertext it never opens, and compares a digest of six digits. Invariant 6
//! says so as a dependency fact rather than a promise: no curve crate is
//! reachable from here, so the code that would break it cannot be written.
//!
//! # Why the orchestration is a service and not a handler
//!
//! `sessions.zig` is six handlers, and every one of them re-derives the same
//! four things from an `httpz.Request` before it can do anything: the peer
//! address, the user agent, a redaction buffer, and a scratch struct declared
//! `undefined` and filled on the next line. The rules about WHICH of those a
//! given verb needs live in whichever handler was written last.
//!
//! Here a verb takes what it needs as parameters of types that already parsed
//! ([`input`]), and the HTTP layer is what turns a request into them. Nothing
//! in this module names a status code, a header, or a request — which is what
//! lets the whole state machine be exercised without one.
//!
//! # Why it lives in this crate
//!
//! It needs the queue, the entropy source and the message authentication code
//! at once, and `afd_auth` deliberately reaches none of them — that crate is
//! the portable half of authentication and stays datastore-free, exactly as
//! `afd_state` exists because it cannot name `sqlx`. This crate is where the
//! daemon's services that DO hold connections already live: the vault, the
//! provider resolver, the money window, the bundle store.

pub mod fingerprint;
pub mod input;

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_crypto::entropy::Entropy;
use afd_crypto::mac::HmacSha256Tag;
use afd_crypto::secret::SecretBytes;
use afd_redis::session::{
    AbortOutcome, AbortReason, ApproveOutcome, SessionState, SessionStore, VerifyOutcome,
};

// Re-exported so the HTTP layer can name the state a poll reports without
// taking a dependency on the queue crate: a status is a fact about the login,
// and the login is this module's.
pub use afd_redis::session::SessionStatus;

use crate::error;
use crate::{Error, Result};

pub use self::fingerprint::Fingerprint;
pub use self::input::{Approval, Opening};

/// The path the dashboard serves a login approval at.
///
/// One constant rather than a format string per call site (RULE UFS): the
/// dashboard route and this daemon's rendering of it are one fact.
const LOGIN_PATH: &str = "/cli-auth/";

/// The device-flow login surface.
///
/// Cheap to clone — a connection handle, a key, an entropy source and a string —
/// which is what lets the composition root hand one to the router by value.
#[derive(Debug, Clone)]
pub struct Sessions {
    store: SessionStore,
    code_pepper: SecretBytes,
    entropy: Entropy,
    app_url: String,
}

impl Sessions {
    /// Binds the login surface to its queue, its pepper and its dashboard.
    ///
    /// `app_url` is where a person goes to approve; a trailing slash on it is
    /// tolerated here rather than in configuration, because an operator setting
    /// an environment variable should not have to know which spelling this
    /// daemon concatenates.
    #[must_use]
    pub fn new(
        store: SessionStore,
        code_pepper: SecretBytes,
        entropy: Entropy,
        app_url: &str,
    ) -> Self {
        Self {
            store,
            code_pepper,
            entropy,
            app_url: app_url.trim_end_matches('/').to_owned(),
        }
    }

    /// Opens a login, answering the id and the page a person approves it on.
    ///
    /// # Errors
    /// Refuses a public key or a token name the bounds will not take, reports a
    /// host that cannot draw entropy, and reports a queue that would not answer.
    pub async fn open(&self, opening: &Opening<'_>, now: UnixMillis) -> Result<Opened> {
        let session_id = self.mint_id(now)?;
        let state = SessionState {
            session_id: session_id.as_str().to_owned(),
            status: SessionStatus::Pending,
            cli_public_key: opening.public_key.as_str().to_owned(),
            token_name: opening.token_name.as_str().to_owned(),
            dashboard_public_key: None,
            ciphertext: None,
            nonce: None,
            verification_code_hmac_hex: None,
            verification_attempts: 0,
            created_at_ms: now.as_millis(),
            expires_at_ms: now.saturating_add_millis(session_ttl_millis()).as_millis(),
            approved_at_ms: None,
            consumed_at_ms: None,
            aborted_reason: None,
            clerk_user_id: None,
            consumed_client_fingerprint_hex: None,
            consume_payload_expires_at_ms: None,
        };
        self.store.put(&state).await?;

        Ok(Opened {
            login_url: format!("{}{LOGIN_PATH}{}", self.app_url, state.session_id),
            session_id: state.session_id,
        })
    }

    /// Reads where a login has got to.
    ///
    /// The three terminal states are ERRORS rather than a status a caller
    /// renders, which is the shape `innerPollAuthSession` already has: a
    /// consumed session is a 410 and a pending one is a 200, and collapsing
    /// them into one success type would push that decision into every caller.
    ///
    /// # Errors
    /// Refuses an id naming nothing held, and each terminal state with its own
    /// registry code. Reports a queue that would not answer.
    pub async fn poll(&self, session_id: &str) -> Result<Waiting> {
        let state = self.load(session_id).await?;
        match state.status {
            SessionStatus::Pending | SessionStatus::VerificationPending => Ok(Waiting {
                status: state.status,
                cli_public_key: state.cli_public_key,
                token_name: state.token_name,
                expires_at_ms: state.expires_at_ms,
            }),
            SessionStatus::Consumed => Err(error::session_consumed()),
            SessionStatus::Expired => Err(error::session_expired()),
            SessionStatus::Aborted => Err(error::session_aborted()),
        }
    }

    /// Records one dashboard approval, once.
    ///
    /// The digest of the code is computed HERE and the six digits never leave
    /// this frame: what the queue holds is a peppered message authentication
    /// code, so a queue somebody can read is not a queue somebody can log in
    /// from.
    ///
    /// # Errors
    /// Refuses a field the bounds will not take, an id naming nothing held, and
    /// a session already past pending. Reports a queue that would not answer.
    pub async fn approve(
        &self,
        session_id: &str,
        approval: &Approval<'_>,
        approver: &str,
        now: UnixMillis,
    ) -> Result<()> {
        let id = parse_id(session_id)?;
        let digest = self.code_digest(id.as_str(), approval.verification_code.as_str());
        let outcome = self
            .store
            .approve(
                &afd_redis::session::Approval {
                    session_id: id.as_str(),
                    dashboard_public_key: approval.dashboard_public_key.as_str(),
                    ciphertext: approval.ciphertext.as_str(),
                    nonce: approval.nonce.as_str(),
                    code_hmac_hex: &digest,
                    approver,
                },
                now.as_millis(),
            )
            .await?;

        match outcome {
            ApproveOutcome::Approved => Ok(()),
            ApproveOutcome::Missing => Err(error::session_missing()),
            ApproveOutcome::Conflict(status) => {
                // The status the script found goes to the log and not the wire:
                // an approver learning that the session it lost the race for is
                // `consumed` rather than `verification_pending` learns whether
                // somebody completed a login, which is not its business.
                tracing::debug!(
                    found = status.as_deref().unwrap_or("unknown"),
                    event = "auth_session_approve_conflict",
                );
                Err(error::session_already_approved())
            }
        }
    }

    /// Presents a code, redeeming the session if it matches.
    ///
    /// # Errors
    /// Refuses a code that is not six digits before computing anything over it,
    /// an id naming nothing held, every terminal state, a session no human has
    /// approved, and a code that did not match — the last two separately,
    /// because one says wait and the other says try again. Reports a queue that
    /// would not answer.
    pub async fn verify(
        &self,
        session_id: &str,
        code: &input::Code<'_>,
        fingerprint: &Fingerprint,
        now: UnixMillis,
    ) -> Result<Redeemed> {
        let id = parse_id(session_id)?;
        let digest = self.code_digest(id.as_str(), code.as_str());
        let outcome = self
            .store
            .verify_and_consume(id.as_str(), &digest, now.as_millis(), fingerprint.as_str())
            .await?;

        match outcome {
            VerifyOutcome::Success(payload) => Ok(Redeemed::first(payload)),
            VerifyOutcome::Replay(payload) => Ok(Redeemed::repeated(payload)),
            VerifyOutcome::Missing => Err(error::session_missing()),
            VerifyOutcome::Expired => Err(error::session_expired()),
            VerifyOutcome::Consumed => Err(error::session_consumed()),
            // The stored reason is not relayed. It is a closed set of three and
            // every one of them has the same remedy — log in again — so the
            // sentence is fixed and the reason rides the queue for an operator.
            VerifyOutcome::Aborted(reason) => {
                tracing::debug!(reason, event = "auth_session_verify_aborted");
                Err(error::session_aborted())
            }
            VerifyOutcome::NotApproved => Err(error::session_not_approved()),
            VerifyOutcome::InvalidCode(attempts) => {
                tracing::debug!(attempts, event = "auth_session_verify_rejected");
                Err(error::session_code_rejected())
            }
            // Its own refusal rather than the retryable one: this attempt tripped
            // the ceiling, so a command line that kept prompting would burn a
            // person's remaining patience on a session that is already dead.
            VerifyOutcome::RateLimited => Err(error::session_rate_limited()),
        }
    }

    /// Cancels one login, if `owner` is the identity that approved it.
    ///
    /// Answers whether THIS call performed the abort, so a caller writes an
    /// audit record on the transition rather than on every idempotent repeat.
    ///
    /// # Errors
    /// Refuses an id naming nothing held, an identity that does not hold the
    /// session, and a session already redeemed. Reports a queue that would not
    /// answer.
    pub async fn cancel(&self, session_id: &str, owner: &str) -> Result<Cancelled> {
        let id = parse_id(session_id)?;
        let outcome = self
            .store
            .abort(id.as_str(), owner, AbortReason::ExplicitCancel)
            .await?;
        match outcome {
            AbortOutcome::Aborted => Ok(Cancelled::Now),
            AbortOutcome::AlreadyAborted => Ok(Cancelled::Already),
            AbortOutcome::Missing => Err(error::session_missing()),
            AbortOutcome::NotOwner => Err(error::session_not_owner()),
            AbortOutcome::Consumed => Err(error::session_consumed()),
        }
    }

    /// Cancels every in-flight login `owner` holds, answering their ids.
    ///
    /// The ids rather than a count, so the caller can write one audit record
    /// per aborted session — the observer-callback-through-`*anyopaque` shape
    /// the Zig store needs for the same thing, expressed as a return value.
    ///
    /// # Errors
    /// Reports a queue that would not answer.
    pub async fn cancel_all(&self, owner: &str) -> Result<Vec<String>> {
        Ok(self
            .store
            .abort_all_for_owner(owner, AbortReason::ExplicitCancel)
            .await?)
    }

    /// Reads one session, refusing an id that names nothing held.
    async fn load(&self, session_id: &str) -> Result<SessionState> {
        let id = parse_id(session_id)?;
        self.store
            .get(id.as_str())
            .await?
            .ok_or_else(error::session_missing)
    }

    /// The peppered digest of one session's code.
    ///
    /// The session id is bound into the message, so a code lifted from one
    /// session cannot be replayed against another even when the two happen to
    /// show the same six digits.
    fn code_digest(&self, session_id: &str, code: &str) -> String {
        HmacSha256Tag::compute_peppered(
            &self.code_pepper,
            &[session_id.as_bytes(), code.as_bytes()],
        )
        .to_hex()
    }

    /// Draws a fresh session identifier.
    fn mint_id(&self, now: UnixMillis) -> Result<Uuid7> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy.fill(&mut bytes)?;
        Ok(Uuid7::encode(now, bytes)?)
    }
}

/// The session time-to-live in milliseconds, from the store's own duration.
///
/// Derived rather than declared a second time: the queue expires the key on one
/// value and the blob carries the same instant, and two constants would let
/// them drift by exactly the amount nobody notices.
fn session_ttl_millis() -> i64 {
    i64::try_from(afd_redis::session::SESSION_TTL.as_millis()).unwrap_or(i64::MAX)
}

/// Refuses a session id that is not a version 7 identifier.
///
/// Answered as "not found" rather than "malformed", which is the Zig
/// `formatSessionKey` behaviour and the right one: a caller holding a
/// syntactically wrong id and one holding a well-formed id for a session that
/// expired are in the same position, and telling them apart would make the poll
/// an oracle for the id shape.
fn parse_id(session_id: &str) -> Result<Uuid7> {
    Uuid7::parse(session_id).map_err(|_shape| error::session_missing())
}

/// A login that has just been opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// The identifier the command line polls on.
    pub session_id: String,
    /// The page a person approves it on.
    pub login_url: String,
}

/// A login that is still in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// Whether it is waiting for a human or for a code.
    pub status: SessionStatus,
    /// The public key the command line presented, echoed back to it.
    pub cli_public_key: String,
    /// What the credential will be called.
    pub token_name: String,
    /// When the window closes, in milliseconds since the epoch.
    pub expires_at_ms: i64,
}

/// A redeemed login's sealed credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redeemed {
    /// The dashboard's public key, for the client's key agreement.
    pub dashboard_public_key: String,
    /// The sealed credential.
    pub ciphertext: String,
    /// The nonce it was sealed under.
    pub nonce: String,
    /// Whether this was a repeat inside the replay window.
    ///
    /// Carried so the caller can tell the two apart in its audit record. The
    /// WIRE shape is identical for both, deliberately: a command line asking
    /// again after a dropped connection must not be able to tell that its first
    /// request landed, or it would have to decide what to do about it.
    pub repeated: bool,
}

impl Redeemed {
    /// The first redemption of a session.
    fn first(payload: afd_redis::session::VerifyPayload) -> Self {
        Self::of(payload, false)
    }

    /// A repeat inside the replay window.
    fn repeated(payload: afd_redis::session::VerifyPayload) -> Self {
        Self::of(payload, true)
    }

    fn of(payload: afd_redis::session::VerifyPayload, repeated: bool) -> Self {
        Self {
            dashboard_public_key: payload.dashboard_public_key,
            ciphertext: payload.ciphertext,
            nonce: payload.nonce,
            repeated,
        }
    }
}

/// Whether a cancel transitioned the session or found it already terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cancelled {
    /// This call performed the abort — the caller writes the audit record.
    Now,
    /// It was already aborted, so nothing changed and nothing is recorded.
    Already,
}

/// The refusal type this module answers with.
pub type SessionError = Error;
