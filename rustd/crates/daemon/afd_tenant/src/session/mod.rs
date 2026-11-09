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
//! Six handlers that each re-derive the same things from a request before they
//! can do anything — the peer address, the user agent, a redaction buffer —
//! leave the rules about WHICH of those a given verb needs in whichever handler
//! was written last.
//!
//! Here a verb takes what it needs as parameters of types that already parsed
//! ([`input`]), and the HTTP layer is what turns a request into them. Nothing
//! in this module names a status code, a header, or a request — which is what
//! lets the whole state machine be exercised without one.
//!
//! # Why it lives in this crate
//!
//! It needs the database, the entropy source and the message authentication code
//! at once, and `afd_auth` deliberately reaches none of them — that crate is
//! the portable half of authentication and stays datastore-free, exactly as
//! `afd_state` exists because it cannot name `sqlx`. This crate is where the
//! daemon's services that DO hold connections already live: the vault, the
//! provider resolver, the money window, the bundle store.

pub mod fingerprint;
pub mod input;
mod state;
mod store;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_crypto::mac::HmacSha256Tag;
use af_crypto::secret::SecretBytes;
use jiff::Timestamp;

pub use self::state::SessionStatus;
use self::state::{AbortReason, ApproveOutcome, VerifyOutcome};
pub use self::store::{CONSUME_REPLAY_WINDOW, DeviceSessions, MAX_VERIFY_ATTEMPTS, SESSION_TTL};

use crate::Result;
use crate::error;

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
    store: DeviceSessions,
    code_pepper: SecretBytes,
    entropy: Entropy,
    app_url: String,
}

impl Sessions {
    /// Binds the login surface to its store, its pepper and its dashboard.
    ///
    /// `app_url` is where a person goes to approve; a trailing slash on it is
    /// tolerated here rather than in configuration, because an operator setting
    /// an environment variable should not have to know which spelling this
    /// daemon concatenates.
    #[must_use]
    pub fn new(
        store: DeviceSessions,
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
    /// host that cannot draw entropy, and reports a store that would not answer.
    pub async fn open(&self, opening: &Opening<'_>, now: Timestamp) -> Result<Opened> {
        let session_id = self.entropy.uuid7(now)?;
        self.store
            .open(
                &session_id,
                opening.public_key.as_str(),
                opening.token_name.as_str(),
            )
            .await?;
        let session_id = session_id.as_str().to_owned();
        Ok(Opened {
            login_url: format!("{}{LOGIN_PATH}{session_id}", self.app_url),
            session_id,
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
    /// registry code. Reports a store that would not answer.
    pub async fn poll(&self, session_id: &str) -> Result<Waiting> {
        let id = parse_id(session_id)?;
        let polled = self
            .store
            .get(&id)
            .await?
            .ok_or_else(error::session_missing)?;
        let status = SessionStatus::parse(&polled.status)
            .ok_or_else(|| error::session_unreadable(format!("status {}", polled.status)))?;
        match status {
            SessionStatus::Pending | SessionStatus::VerificationPending => Ok(Waiting {
                status,
                cli_public_key: polled.cli_public_key,
                token_name: polled.token_name,
                expires_at: polled.expires_at,
            }),
            SessionStatus::Consumed => Err(error::session_consumed()),
            SessionStatus::Expired => Err(error::session_expired()),
            SessionStatus::Aborted => Err(error::session_aborted()),
        }
    }

    /// Records one dashboard approval, once.
    ///
    /// The digest of the code is computed HERE and the six digits never leave
    /// this frame: what the row holds is a peppered message authentication
    /// code, so a table somebody can read is not a table somebody can log in
    /// from.
    ///
    /// # Errors
    /// Refuses a field the bounds will not take, an id naming nothing held, and
    /// a session already past pending. Reports a store that would not answer.
    pub async fn approve(
        &self,
        session_id: &str,
        approval: &Approval<'_>,
        approver: &str,
    ) -> Result<()> {
        let id = parse_id(session_id)?;
        let digest = self.code_digest(id.as_str(), approval.verification_code.as_str());
        let outcome = self
            .store
            .approve(
                &id,
                &state::Approval {
                    dashboard_public_key: approval.dashboard_public_key.as_str(),
                    ciphertext: approval.ciphertext.as_str(),
                    nonce: approval.nonce.as_str(),
                    code: &digest,
                    approver,
                },
            )
            .await?;

        match outcome {
            ApproveOutcome::Approved => Ok(()),
            ApproveOutcome::Missing => Err(error::session_missing()),
            ApproveOutcome::Conflict(status) => {
                // The status the statement found goes to the log and not the wire:
                // an approver learning that the session it lost the race for is
                // `consumed` rather than `verification_pending` learns whether
                // somebody completed a login, which is not its business.
                // Hoisted: the `log` bridge duplicates field expressions and
                // llvm-cov scores the dead copy.
                let found = status.map_or("unknown", SessionStatus::as_str);
                tracing::debug!(found, event = "auth_session_approve_conflict");
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
    /// because one says wait and the other says try again. Reports a store that
    /// would not answer.
    pub async fn verify(
        &self,
        session_id: &str,
        code: &input::Code<'_>,
        fingerprint: &Fingerprint,
    ) -> Result<Redeemed> {
        let id = parse_id(session_id)?;
        let digest = self.code_digest(id.as_str(), code.as_str());
        let outcome = self
            .store
            .verify_and_consume(&id, &digest, fingerprint)
            .await?;

        match outcome {
            VerifyOutcome::Success(payload) => Ok(Redeemed::first(payload)),
            VerifyOutcome::Replay(payload) => Ok(Redeemed::repeated(payload)),
            VerifyOutcome::Missing => Err(error::session_missing()),
            VerifyOutcome::Expired => Err(error::session_expired()),
            VerifyOutcome::Consumed => Err(error::session_consumed()),
            // The stored reason is not relayed. It is a closed set of three and
            // every one of them has the same remedy — log in again — so the
            // sentence is fixed and the reason stays on the row for an operator.
            VerifyOutcome::Aborted(reason) => {
                let reason = reason.map(AbortReason::as_str);
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

    /// Cancels every in-flight login `owner` holds, answering their ids.
    ///
    /// The ids rather than a count, so the caller can write one audit record
    /// per aborted session without the store taking an observer callback.
    ///
    /// # Errors
    /// Reports a store that would not answer.
    pub async fn cancel_all(&self, owner: &str) -> Result<Vec<String>> {
        self.store
            .abort_all_for_owner(owner, AbortReason::ExplicitCancel)
            .await
    }

    /// The peppered digest of one session's code.
    ///
    /// The session id is bound into the message, so a code lifted from one
    /// session cannot be replayed against another even when the two happen to
    /// show the same six digits.
    fn code_digest(&self, session_id: &str, code: &str) -> HmacSha256Tag {
        HmacSha256Tag::compute_peppered(
            &self.code_pepper,
            &[session_id.as_bytes(), code.as_bytes()],
        )
    }
}

/// Refuses a session id that is not a version 7 identifier.
///
/// Answered as "not found" rather than "malformed", because a caller holding a
/// syntactically wrong id and one holding a well-formed id for a session that
/// expired are in the same position, and telling them apart would make the poll
/// an oracle for the id shape.
fn parse_id(session_id: &str) -> Result<Uuid7> {
    Uuid7::parse(session_id).map_err(|_shape| error::session_missing())
}

mod response;
pub use self::response::{Opened, Redeemed, Waiting};
