//! Device-login sessions as rows in `identity.device_sessions`.
//!
//! # One locked transaction per verify
//!
//! The retired Lua script read, branched and wrote one key atomically. Here
//! the verify locks the row, reads the clock once after the lock, classifies
//! in Rust, compares the code digest in constant time, writes one `UPDATE`
//! and commits before the handler answers. Concurrent verifies of one session
//! queue on the row lock, so five wrong codes abort it exactly once and a
//! correct code redeems exactly once.

use std::time::Duration;

use af_core::id::Uuid7;
use af_crypto::mac::HmacSha256Tag;
use afd_db::Db;
use jiff::Timestamp;

use super::fingerprint::Fingerprint;
use super::state::{
    AbortReason, Approval, ApproveOutcome, SessionStatus, VerifyOutcome, VerifyPayload,
};
use crate::Result;
use crate::error::query;
use crate::sql::session as sql;

/// A session's life, re-stamped at every transition.
pub const SESSION_TTL: Duration = Duration::from_secs(300);
/// How long a redeemed session answers the same request again.
pub const CONSUME_REPLAY_WINDOW: Duration = Duration::from_secs(60);
/// Wrong codes a session survives before it aborts.
pub const MAX_VERIFY_ATTEMPTS: u8 = 5;

const CONTEXT_OPEN: &str = "open a device session";
const CONTEXT_READ: &str = "read a device session";
const CONTEXT_VERIFY: &str = "verify a device session";
const CONTEXT_APPROVE: &str = "approve a device session";
const CONTEXT_ABORT: &str = "abort device sessions";
const CONTEXT_SWEEP: &str = "sweep expired device sessions";

/// The store every device-login transition goes through.
#[derive(Debug, Clone)]
pub struct SessionStore {
    database: Db,
}

/// A live session as a poll sees it.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct Polled {
    pub(crate) status: String,
    pub(crate) cli_public_key: String,
    pub(crate) token_name: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub(crate) expires_at: Timestamp,
}

/// A session under its row lock, with the clock read once after the lock.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct Locked {
    pub(crate) status: String,
    pub(crate) dashboard_public_key: Option<String>,
    pub(crate) ciphertext: Option<String>,
    pub(crate) nonce: Option<String>,
    pub(crate) verification_code_hmac: Option<Vec<u8>>,
    pub(crate) verification_attempts: i16,
    pub(crate) aborted_reason: Option<String>,
    pub(crate) consumed_client_fingerprint: Option<Vec<u8>>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub(crate) expires_at: Timestamp,
    pub(crate) replay_expires_at: Option<jiff_sqlx::Timestamp>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub(crate) now: Timestamp,
}

/// The one write a verify makes, if any.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Write {
    /// A wrong code: the new count, and the abort at the cap.
    Attempt {
        attempts: u8,
        status: SessionStatus,
        reason: Option<AbortReason>,
    },
    /// The correct code: redeem at the locked clock.
    Consume,
}

impl SessionStore {
    /// Binds the store to the runtime pool.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }

    /// Writes a new `pending` session and answers its deadline.
    pub(crate) async fn open(
        &self,
        id: &Uuid7,
        cli_public_key: &str,
        token_name: &str,
    ) -> Result<Timestamp> {
        let mut connection = self.database.acquire().await?;
        let expires_at = sqlx::query_scalar::<_, jiff_sqlx::Timestamp>(sql::INSERT_SESSION)
            .bind(id)
            .bind(SessionStatus::Pending.as_str())
            .bind(cli_public_key)
            .bind(token_name)
            .bind(SESSION_TTL)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_OPEN))?;
        Ok(expires_at.to_jiff())
    }

    /// A live session, or `None` once it is gone or past its deadline.
    pub(crate) async fn get(&self, id: &Uuid7) -> Result<Option<Polled>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query_as::<_, Polled>(sql::SELECT_SESSION)
            .bind(id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_READ))
    }

    /// Verifies `submitted` and redeems the session, in one locked transaction.
    pub(crate) async fn verify_and_consume(
        &self,
        id: &Uuid7,
        submitted: &HmacSha256Tag,
        fingerprint: &Fingerprint,
    ) -> Result<VerifyOutcome> {
        let mut transaction = sqlx::Transaction::begin(self.database.acquire().await?, None)
            .await
            .map_err(query(CONTEXT_VERIFY))?;
        let Some(row) = sqlx::query_as::<_, Locked>(sql::LOCK_SESSION)
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_VERIFY))?
        else {
            return Ok(VerifyOutcome::Missing);
        };
        let (outcome, write) = classify(&row, submitted, fingerprint)?;
        match write {
            None => return Ok(outcome),
            Some(Write::Attempt {
                attempts,
                status,
                reason,
            }) => {
                sqlx::query(sql::RECORD_ATTEMPT)
                    .bind(id)
                    .bind(i16::from(attempts))
                    .bind(status.as_str())
                    .bind(reason.map(AbortReason::as_str))
                    .bind(SESSION_TTL)
                    .execute(&mut *transaction)
                    .await
                    .map_err(query(CONTEXT_VERIFY))?;
            }
            Some(Write::Consume) => {
                sqlx::query(sql::CONSUME_SESSION)
                    .bind(id)
                    .bind(SessionStatus::Consumed.as_str())
                    .bind(jiff_sqlx::Timestamp::from(row.now))
                    .bind(CONSUME_REPLAY_WINDOW)
                    .bind(fingerprint.as_bytes().as_slice())
                    .bind(SESSION_TTL)
                    .execute(&mut *transaction)
                    .await
                    .map_err(query(CONTEXT_VERIFY))?;
            }
        }
        // Committed before the handler answers: a response the client acts on
        // never describes a transition that could still roll back.
        transaction.commit().await.map_err(query(CONTEXT_VERIFY))?;
        Ok(outcome)
    }

    /// Approves a live `pending` session in one conditional statement.
    pub(crate) async fn approve(
        &self,
        id: &Uuid7,
        approval: &Approval<'_>,
    ) -> Result<ApproveOutcome> {
        let mut connection = self.database.acquire().await?;
        let approved = sqlx::query_scalar::<_, Uuid7>(sql::APPROVE_SESSION)
            .bind(id)
            .bind(SessionStatus::Pending.as_str())
            .bind(SessionStatus::VerificationPending.as_str())
            .bind(approval.dashboard_public_key)
            .bind(approval.ciphertext)
            .bind(approval.nonce)
            .bind(approval.code.as_bytes().as_slice())
            .bind(approval.approver)
            .bind(SESSION_TTL)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_APPROVE))?;
        if approved.is_some() {
            return Ok(ApproveOutcome::Approved);
        }
        let found = sqlx::query_scalar::<_, String>(sql::SELECT_SESSION_STATUS)
            .bind(id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_APPROVE))?;
        Ok(found.map_or(ApproveOutcome::Missing, |status| {
            ApproveOutcome::Conflict(SessionStatus::parse(&status))
        }))
    }

    /// Aborts every live, unredeemed session `owner` approved.
    pub(crate) async fn abort_all_for_owner(
        &self,
        owner: &str,
        reason: AbortReason,
    ) -> Result<Vec<String>> {
        let mut connection = self.database.acquire().await?;
        let terminal = [
            SessionStatus::Consumed.as_str(),
            SessionStatus::Aborted.as_str(),
        ];
        let aborted = sqlx::query_scalar::<_, Uuid7>(sql::ABORT_OWNED_SESSIONS)
            .bind(owner)
            .bind(terminal.as_slice())
            .bind(SessionStatus::Aborted.as_str())
            .bind(reason.as_str())
            .bind(SESSION_TTL)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_ABORT))?;
        Ok(aborted.iter().map(|id| id.as_str().to_owned()).collect())
    }

    /// Deletes up to `limit` expired sessions; answers how many went.
    ///
    /// # Errors
    /// Reports a store that would not answer; the next pass retries.
    pub async fn sweep_expired(&self, limit: i64) -> Result<u64> {
        let mut connection = self.database.acquire().await?;
        let swept = sqlx::query(sql::DELETE_EXPIRED_SESSIONS)
            .bind(limit)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_SWEEP))?;
        Ok(swept.rows_affected())
    }
}

/// Decides a verify from the locked row, exactly as the retired script did,
/// with one change: a row past its deadline answers `Expired` where a lapsed
/// key answered `Missing`.
pub(crate) fn classify(
    row: &Locked,
    submitted: &HmacSha256Tag,
    fingerprint: &Fingerprint,
) -> Result<(VerifyOutcome, Option<Write>)> {
    if row.now >= row.expires_at {
        return Ok((VerifyOutcome::Expired, None));
    }
    let status = SessionStatus::parse(&row.status).ok_or_else(unreadable)?;
    let outcome = match status {
        SessionStatus::Consumed => {
            let open = row
                .replay_expires_at
                .is_some_and(|until| until.to_jiff() > row.now);
            let same = row.consumed_client_fingerprint.as_deref()
                == Some(fingerprint.as_bytes().as_slice());
            if open && same {
                VerifyOutcome::Replay(payload(row)?)
            } else {
                VerifyOutcome::Consumed
            }
        }
        SessionStatus::Expired => VerifyOutcome::Expired,
        SessionStatus::Aborted => VerifyOutcome::Aborted(
            row.aborted_reason
                .clone()
                .unwrap_or_else(|| "unknown".to_owned()),
        ),
        SessionStatus::Pending => VerifyOutcome::NotApproved,
        SessionStatus::VerificationPending => return compare(row, submitted),
    };
    Ok((outcome, None))
}

fn compare(row: &Locked, submitted: &HmacSha256Tag) -> Result<(VerifyOutcome, Option<Write>)> {
    let matches = row
        .verification_code_hmac
        .as_deref()
        .and_then(|stored| HmacSha256Tag::from_slice(stored).ok())
        .is_some_and(|stored| stored.verify(submitted).is_ok());
    if matches {
        return Ok((VerifyOutcome::Success(payload(row)?), Some(Write::Consume)));
    }
    let attempts = u8::try_from(row.verification_attempts)
        .unwrap_or(MAX_VERIFY_ATTEMPTS)
        .saturating_add(1);
    if attempts >= MAX_VERIFY_ATTEMPTS {
        let write = Write::Attempt {
            attempts,
            status: SessionStatus::Aborted,
            reason: Some(AbortReason::RateLimitExceeded),
        };
        return Ok((VerifyOutcome::RateLimited, Some(write)));
    }
    let write = Write::Attempt {
        attempts,
        status: SessionStatus::VerificationPending,
        reason: None,
    };
    Ok((VerifyOutcome::InvalidCode(attempts), Some(write)))
}

fn payload(row: &Locked) -> Result<VerifyPayload> {
    match (&row.dashboard_public_key, &row.ciphertext, &row.nonce) {
        (Some(dashboard_public_key), Some(ciphertext), Some(nonce)) => Ok(VerifyPayload {
            dashboard_public_key: dashboard_public_key.clone(),
            ciphertext: ciphertext.clone(),
            nonce: nonce.clone(),
        }),
        _ => Err(unreadable()),
    }
}

/// A row whose columns are not a shape this daemon writes.
fn unreadable() -> crate::Error {
    query(CONTEXT_VERIFY)(sqlx::Error::Protocol(
        "a device session row this daemon never writes".to_owned(),
    ))
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
