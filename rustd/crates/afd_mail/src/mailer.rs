//! The invite send: the relay read from the admin workspace's vault, the
//! message built and delivered under one deadline, and the outcome logged.
//!
//! Every way this can go wrong is an [`Outcome`], never an error: the invite
//! already exists when this runs, and the caller records what happened against
//! it. The send crosses a datastore and a network boundary, so it logs
//! `invite_email_started` on entry and exactly one of `invite_email_completed`
//! or `invite_email_failed` on every exit (`docs/LOGGING_STANDARD.md` §4). Each
//! record carries the invite id, the attempt and the relay's reply code —
//! never the recipient's address or the message body (§6).

use std::time::{Duration, Instant};

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_vault::{SecretName, Vault};
use lettre::message::Mailbox;

use crate::deliver::{self, Attempted, Delivery, IdempotencyKey, Mailer};
use crate::relay::{Relay, SMTP_RELAY_BAG};
use crate::{InviteLetter, render_invite};

/// How long one invite send may take: the relay read and both tries.
///
/// Creating an invite waits for it, so this bounds what a slow relay costs the
/// owner's click; past it the invite records `failed` and the owner can send
/// again.
pub const MAIL_SEND_DEADLINE: Duration = Duration::from_secs(10);

const EVENT_STARTED: &str = "invite_email_started";
const EVENT_COMPLETED: &str = "invite_email_completed";
const EVENT_FAILED: &str = "invite_email_failed";
const EVENT_RETRIED: &str = "invite_email_retried";

/// Why a send failed, as the `reason` field spells it.
const REASON_UNCONFIGURED: &str = "unconfigured";
const REASON_RELAY_UNREAD: &str = "relay_unread";
const REASON_TLS_SETUP: &str = "tls_setup";
const REASON_UNBUILDABLE: &str = "message_unbuildable";
const REASON_REFUSED: &str = "refused";
const REASON_UNREACHABLE: &str = "unreachable";
const REASON_DEADLINE: &str = "deadline";

/// What became of one invite email.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The relay accepted it, with this reply code.
    Sent {
        /// The relay's SMTP reply code.
        reply: u16,
    },
    /// The relay refused it, never answered, or the deadline passed; `reply`
    /// is the relay's code when it spoke one.
    Failed {
        /// The relay's SMTP reply code, when it answered.
        reply: Option<u16>,
    },
    /// No usable `smtp-relay` bag, or no admin workspace to hold one.
    Unconfigured,
}

/// One invite email to send.
#[derive(Debug, Clone, Copy)]
pub struct InviteSend<'a> {
    /// The invite, which the idempotency key and every event name.
    pub invite_id: &'a Uuid7,
    /// The attempt number, already committed against the invite.
    pub attempt: i32,
    /// The invitee's address.
    pub to: &'a str,
    /// What the email says.
    pub letter: InviteLetter<'a>,
}

/// Sends invite emails through the relay the admin workspace's vault names.
#[derive(Debug, Clone)]
pub struct InviteMailer {
    vault: Vault,
    deadline: Duration,
}

impl InviteMailer {
    /// A mailer reading the relay from `vault`, under [`MAIL_SEND_DEADLINE`].
    #[must_use]
    pub const fn new(vault: Vault) -> Self {
        Self {
            vault,
            deadline: MAIL_SEND_DEADLINE,
        }
    }

    /// The same mailer under another deadline, for a suite proving the stall.
    #[must_use]
    pub const fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    /// Sends one invite email and reports what became of it.
    pub async fn send(&self, admin: Option<&Uuid7>, invite: &InviteSend<'_>) -> Outcome {
        send_within(self.relay(admin), Relay::transport, invite, self.deadline).await
    }

    /// The relay, when the admin workspace holds a usable bag.
    async fn relay(&self, admin: Option<&Uuid7>) -> afd_vault::Result<Option<Relay>> {
        let (Some(admin), Ok(name)) = (admin, SecretName::parse(SMTP_RELAY_BAG)) else {
            return Ok(None);
        };
        let stored = self.vault.load(admin, &name).await?;
        Ok(stored.as_ref().and_then(Relay::parse))
    }
}

/// Reads the relay, then sends through it, all under `deadline`.
///
/// The read is a future rather than the vault itself so a suite can hand it
/// one that never resolves. `connect` is [`Relay::transport`] in production
/// and a parameter so a suite can hand it a TLS setup that fails: no relay
/// provokes one on a host whose system trust store loads.
pub(crate) async fn send_within<M: Mailer>(
    read: impl Future<Output = afd_vault::Result<Option<Relay>>>,
    connect: impl FnOnce(&Relay, Duration) -> crate::Result<M, lettre::transport::smtp::Error>,
    invite: &InviteSend<'_>,
    deadline: Duration,
) -> Outcome {
    let attempt = Attempt::begin(invite);
    let until = tokio::time::Instant::now() + deadline;
    let relay = match tokio::time::timeout_at(until, read).await {
        Ok(Ok(Some(relay))) => relay,
        Ok(Ok(None)) => return attempt.unconfigured(),
        Ok(Err(error)) => return attempt.failed(None, REASON_RELAY_UNREAD, Some(&error)),
        Err(_elapsed) => return attempt.failed(None, REASON_DEADLINE, None),
    };
    // Whatever the read spent comes out of what the relay gets.
    let remaining = until.saturating_duration_since(tokio::time::Instant::now());
    match connect(&relay, remaining) {
        Ok(transport) => send_with(&transport, &relay.from, &attempt, remaining).await,
        Err(error) => attempt.failed(None, REASON_TLS_SETUP, Some(&error)),
    }
}

/// Builds the message, delivers it under `deadline`, and logs the outcome.
pub(crate) async fn send_with<M: Mailer>(
    mailer: &M,
    from: &Mailbox,
    attempt: &Attempt<'_>,
    deadline: Duration,
) -> Outcome {
    let message = match build(from, attempt.invite) {
        Ok(message) => message,
        Err(error) => return attempt.failed(None, REASON_UNBUILDABLE, Some(&error)),
    };
    let Ok(attempted) =
        tokio::time::timeout(deadline, deliver::send_once_retrying(mailer, message)).await
    else {
        return attempt.failed(None, REASON_DEADLINE, None);
    };
    attempt.retried(attempted);
    match attempted.delivery {
        Delivery::Accepted { reply } => attempt.completed(reply),
        Delivery::Refused { reply } => attempt.failed(reply, REASON_REFUSED, None),
        Delivery::Unreachable => attempt.failed(None, REASON_UNREACHABLE, None),
    }
}

/// Whether an invite email can be addressed to `address`.
///
/// The parser `build` runs on every recipient, so the invite route refuses an
/// address here rather than storing an invite whose every send would fail.
#[must_use]
pub fn deliverable(address: &str) -> bool {
    address.parse::<lettre::Address>().is_ok()
}

fn build(from: &Mailbox, invite: &InviteSend<'_>) -> crate::Result<lettre::Message> {
    let rendered = render_invite(&invite.letter)?;
    let to: Mailbox = invite.to.parse()?;
    let key = IdempotencyKey::for_invite(invite.invite_id.as_str(), invite.attempt);
    deliver::message(from.clone(), to, &rendered, key)
}

/// One send in flight: what its records correlate on, and when it began.
pub(crate) struct Attempt<'a> {
    invite: &'a InviteSend<'a>,
    began: Instant,
}

impl<'a> Attempt<'a> {
    /// Logs the start of a send and opens its pair.
    pub(crate) fn begin(invite: &'a InviteSend<'a>) -> Self {
        let invite_id = invite.invite_id.as_str();
        let attempt = invite.attempt;
        tracing::info!(invite_id, attempt, event = EVENT_STARTED);
        Self {
            invite,
            began: Instant::now(),
        }
    }

    fn duration_ms(&self) -> u64 {
        u64::try_from(self.began.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn completed(&self, reply: u16) -> Outcome {
        let invite_id = self.invite.invite_id.as_str();
        let attempt = self.invite.attempt;
        let duration_ms = self.duration_ms();
        tracing::info!(
            invite_id,
            attempt,
            reply,
            duration_ms,
            event = EVENT_COMPLETED
        );
        Outcome::Sent { reply }
    }

    /// A send that did not reach the invitee: `warn`, because the invite
    /// stands and the owner can send again.
    fn failed(
        &self,
        reply: Option<u16>,
        reason: &str,
        cause: Option<&dyn std::error::Error>,
    ) -> Outcome {
        let code = error_code::INVITE_EMAIL_UNAVAILABLE.as_str();
        let invite_id = self.invite.invite_id.as_str();
        let attempt = self.invite.attempt;
        let duration_ms = self.duration_ms();
        let detail = cause.map(ToString::to_string);
        tracing::warn!(
            error_code = code,
            invite_id,
            attempt,
            reason,
            reply,
            duration_ms,
            detail,
            event = EVENT_FAILED
        );
        Outcome::Failed { reply }
    }

    /// No relay to send through: `err`, because an operator has to act — run
    /// the `smtp_relay_registration` playbook — before any invite email goes.
    fn unconfigured(&self) -> Outcome {
        let code = error_code::INVITE_EMAIL_UNAVAILABLE.as_str();
        let invite_id = self.invite.invite_id.as_str();
        let attempt = self.invite.attempt;
        let duration_ms = self.duration_ms();
        let reason = REASON_UNCONFIGURED;
        tracing::error!(
            error_code = code,
            invite_id,
            attempt,
            reason,
            bag = SMTP_RELAY_BAG,
            duration_ms,
            event = EVENT_FAILED
        );
        Outcome::Unconfigured
    }

    /// A first try that never reached the relay, recovered or not: `warn`,
    /// degraded behaviour on a recoverable path.
    fn retried(&self, attempted: Attempted) {
        if attempted.retried {
            let invite_id = self.invite.invite_id.as_str();
            let attempt = self.invite.attempt;
            let recovered = matches!(attempted.delivery, Delivery::Accepted { .. });
            tracing::warn!(invite_id, attempt, recovered, event = EVENT_RETRIED);
        }
    }
}

#[cfg(test)]
mod tests;
