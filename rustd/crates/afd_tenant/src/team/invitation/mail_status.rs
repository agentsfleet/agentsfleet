//! The invite email's half of an invitation: which send this is, and what
//! became of the last one.
//!
//! The send itself is `afd_mail`'s and happens in the route. The store only
//! counts attempts and records results, in that order: [`Team::begin_email`]
//! commits the attempt before the send, so the idempotency key it names is
//! never reused, and [`Team::record_email`] writes the result only while that
//! attempt is still the latest.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::spelling::from_spelling;

use crate::sql::invite as sql;
use crate::team::Team;
use crate::workspace::access::ROLE_OWNER;
use crate::{Result, error};

/// The stored and wire spelling of an email the relay accepted.
pub const EMAIL_STATUS_SENT: &str = "sent";
/// The stored and wire spelling of an email the relay refused or never took.
pub const EMAIL_STATUS_FAILED: &str = "failed";
/// The stored and wire spelling of an email with no relay to go through.
pub const EMAIL_STATUS_UNCONFIGURED: &str = "unconfigured";

const CONTEXT_BEGIN: &str = "begin invite email";
const CONTEXT_RECORD: &str = "record invite email";

/// What became of an invitation's most recent email.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailStatus {
    /// The relay accepted it.
    Sent,
    /// The relay refused it, never answered, or the send never finished.
    Failed,
    /// No relay is set up for this deployment.
    Unconfigured,
}

impl EmailStatus {
    /// The stored and wire spelling.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Sent => EMAIL_STATUS_SENT,
            Self::Failed => EMAIL_STATUS_FAILED,
            Self::Unconfigured => EMAIL_STATUS_UNCONFIGURED,
        }
    }

    /// The status a stored column reads as.
    ///
    /// Total, unlike [`crate::workspace::access::Role::parse`]: nothing is
    /// granted on it. `NULL` is a send that began and never recorded — the
    /// process died between the two — which is `failed` until an owner sends
    /// again. A spelling this build does not know, as a rolled-back build would
    /// meet, reads `failed` too, rather than failing the whole invite list.
    #[must_use]
    pub fn from_stored(stored: Option<&str>) -> Self {
        stored.and_then(from_spelling).unwrap_or(Self::Failed)
    }
}

/// One counted send, and what its email says.
///
/// Decoded by `sqlx::FromRow`, by column name: nothing here parses, so there
/// is no domain error for a hand-written reader to surface.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct EmailAttempt {
    /// Which send this is: the first is 1.
    #[sqlx(rename = "email_attempts")]
    pub attempt: i32,
    /// The invitee's address, lowercased.
    #[sqlx(rename = "email")]
    pub to: String,
    /// The inviter's display name, else their address.
    pub inviter_name: String,
    /// The name the account goes by: its owner's display name, else its own.
    pub owner_name: String,
}

impl Team {
    /// Counts one more send of a pending invitation in `tenant`, and returns
    /// what that send's email says.
    ///
    /// `None` when the invitation is not this account's, or can no longer be
    /// accepted — there is nothing to send.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn begin_email(
        &self,
        tenant: &Uuid7,
        invite: &Uuid7,
        now: UnixMillis,
    ) -> Result<Option<EmailAttempt>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query_as(sql::BEGIN_EMAIL_ATTEMPT)
            .bind(tenant.as_str())
            .bind(invite.as_str())
            .bind(now.as_millis())
            .bind(ROLE_OWNER)
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_BEGIN))
    }

    /// Records what became of `attempt`, unless a later send has begun.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn record_email(
        &self,
        invite: &Uuid7,
        attempt: i32,
        status: EmailStatus,
        now: UnixMillis,
    ) -> Result<()> {
        let sent_at = (status == EmailStatus::Sent).then(|| now.as_millis());
        let mut connection = self.database.acquire().await?;
        sqlx::query(sql::RECORD_EMAIL_RESULT)
            .bind(invite.as_str())
            .bind(attempt)
            .bind(status.wire())
            .bind(sent_at)
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_RECORD))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{EMAIL_STATUS_FAILED, EMAIL_STATUS_SENT, EMAIL_STATUS_UNCONFIGURED, EmailStatus};

    #[test]
    fn every_status_reads_back_as_itself() {
        for status in [
            EmailStatus::Sent,
            EmailStatus::Failed,
            EmailStatus::Unconfigured,
        ] {
            assert_eq!(EmailStatus::from_stored(Some(status.wire())), status);
        }
        assert_eq!(EmailStatus::Sent.wire(), EMAIL_STATUS_SENT);
        assert_eq!(EmailStatus::Failed.wire(), EMAIL_STATUS_FAILED);
        assert_eq!(EmailStatus::Unconfigured.wire(), EMAIL_STATUS_UNCONFIGURED);
    }

    #[test]
    fn an_unrecorded_or_unknown_send_reads_failed() {
        assert_eq!(EmailStatus::from_stored(None), EmailStatus::Failed);
        assert_eq!(
            EmailStatus::from_stored(Some("bounced")),
            EmailStatus::Failed
        );
    }
}
