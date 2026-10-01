//! The invite send: the relay read from the admin workspace's vault, the
//! message built and delivered under one deadline, and the outcome logged.
//!
//! Every way this can go wrong is an [`InviteEmailOutcome`], never an error:
//! the invite already exists when this runs, and the caller records what
//! happened against it. The send crosses a datastore and a network boundary,
//! so it logs `invite_email_started` on entry and exactly one of
//! `invite_email_completed` or `invite_email_failed` on every exit
//! (`docs/LOGGING_STANDARD.md` §4). Each record carries the invite id, the
//! attempt and the relay's reply code — never the recipient's address or the
//! message body (§6).

use std::time::{Duration, Instant};

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_observability::InviteEmailOutcome;
use afd_vault::{SecretName, Vault};
use lettre::message::Mailbox;

use crate::deliver::{self, Attempted, Delivery, IdempotencyKey, Mailer};
use crate::relay::{BagFault, Relay, SMTP_RELAY_BAG, Server, TlsCache};
use crate::{InviteLetter, render_invite};

/// How long one invite send may take: the relay read and both tries.
///
/// Creating an invite waits for it, so this bounds what a slow relay costs the
/// owner's click; past it the invite records `failed` and the owner can send
/// again.
pub(crate) const MAIL_SEND_DEADLINE: Duration = Duration::from_secs(10);

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

/// What reading the relay comes to: the vault's answer, and within it the
/// relay or the fault that leaves the deployment unconfigured.
pub(crate) type RelayRead = afd_vault::Result<crate::Result<Relay, BagFault>>;

/// Sends invite emails through the relay the admin workspace's vault names.
#[derive(Debug, Clone)]
pub struct InviteMailer {
    vault: Vault,
    deadline: Duration,
    tls: TlsCache,
}

impl InviteMailer {
    /// A mailer reading the relay from `vault`, under [`MAIL_SEND_DEADLINE`].
    #[must_use]
    pub fn new(vault: Vault) -> Self {
        Self {
            vault,
            deadline: MAIL_SEND_DEADLINE,
            tls: TlsCache::default(),
        }
    }

    /// The same mailer under another deadline, for a suite proving the stall.
    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub const fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    /// Sends one invite email and reports what became of it.
    pub async fn send(&self, admin: Option<&Uuid7>, invite: &InviteSend<'_>) -> InviteEmailOutcome {
        let connect = |server: Server, timeout| server.transport(timeout, &self.tls);
        send_within(self.relay(admin), connect, invite, self.deadline).await
    }

    /// The relay the admin workspace's bag names, or why there is none.
    async fn relay(&self, admin: Option<&Uuid7>) -> RelayRead {
        let (Some(admin), Ok(name)) = (admin, SecretName::parse(SMTP_RELAY_BAG)) else {
            return Ok(Err(BagFault::Absent));
        };
        let stored = self.vault.load(admin, &name).await?;
        Ok(stored
            .as_ref()
            .ok_or(BagFault::Absent)
            .and_then(Relay::parse))
    }
}

/// Reads the relay, then sends through it, all under `deadline`.
///
/// The read is a future rather than the vault itself so a suite can hand it
/// one that never resolves. `connect` is [`Server::transport`] over the
/// mailer's TLS cache in production, and a parameter so a suite can hand it a
/// TLS setup that fails: no relay provokes one on a host whose system trust
/// store loads.
pub(crate) async fn send_within<M: Mailer>(
    read: impl Future<Output = RelayRead>,
    connect: impl FnOnce(Server, Duration) -> crate::Result<M, lettre::transport::smtp::Error>,
    invite: &InviteSend<'_>,
    deadline: Duration,
) -> InviteEmailOutcome {
    let attempt = Attempt::begin(invite);
    let until = tokio::time::Instant::now() + deadline;
    let Relay { server, from } = match tokio::time::timeout_at(until, read).await {
        Ok(Ok(Ok(relay))) => relay,
        Ok(Ok(Err(fault))) => return attempt.unconfigured(fault),
        Ok(Err(error)) => return attempt.failed(None, REASON_RELAY_UNREAD, Some(&error)),
        Err(_elapsed) => return attempt.failed(None, REASON_DEADLINE, None),
    };
    // Whatever the read spent comes out of what the relay gets.
    let remaining = until.saturating_duration_since(tokio::time::Instant::now());
    match connect(server, remaining) {
        Ok(transport) => send_with(&transport, from, &attempt, remaining).await,
        Err(error) => attempt.failed(None, REASON_TLS_SETUP, Some(&error)),
    }
}

/// Builds the message, delivers it under `deadline`, and logs the outcome.
pub(crate) async fn send_with<M: Mailer>(
    mailer: &M,
    from: Mailbox,
    attempt: &Attempt<'_>,
    deadline: Duration,
) -> InviteEmailOutcome {
    let message = match build(from, attempt.invite) {
        Ok(message) => message,
        Err(error) => return attempt.failed(None, REASON_UNBUILDABLE, Some(&error)),
    };
    let Ok(attempted) =
        tokio::time::timeout(deadline, deliver::send_once_retrying(mailer, &message)).await
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
/// `build` addresses every recipient through [`recipient`], the same check, so
/// an address accepted here is one every send can address, and the invite
/// route refuses any other rather than storing an invite whose every send
/// would fail.
#[must_use]
pub fn deliverable(address: &str) -> bool {
    recipient(address).is_ok()
}

/// The invitee as a message can address them: a bare address, with no display
/// name, that the message builder also carries. The builder derives the
/// envelope by reading the `To` header back with the mailbox parser, which
/// refuses a domain literal (`bob@[10.0.0.1]`) the address parser accepts, so
/// an address has to pass both.
fn recipient(address: &str) -> crate::Result<Mailbox> {
    address.parse::<lettre::Address>()?;
    Ok(address.parse()?)
}

fn build(from: Mailbox, invite: &InviteSend<'_>) -> crate::Result<lettre::Message> {
    let rendered = render_invite(&invite.letter)?;
    let to = recipient(invite.to)?;
    let key = IdempotencyKey::for_invite(invite.invite_id.as_str(), invite.attempt);
    deliver::message(from, to, rendered, key)
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

    fn completed(&self, reply: u16) -> InviteEmailOutcome {
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
        InviteEmailOutcome::Sent { reply }
    }

    /// A send that did not reach the invitee: `warn`, because the invite
    /// stands and the owner can send again.
    fn failed(
        &self,
        reply: Option<u16>,
        reason: &str,
        cause: Option<&dyn std::error::Error>,
    ) -> InviteEmailOutcome {
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
        InviteEmailOutcome::Failed { reply }
    }

    /// No relay to send through: `err`, because an operator has to act — run
    /// the `smtp_relay_registration` playbook — before any invite email goes.
    /// `fault` and `field` say what to fix in the bag, by name only.
    fn unconfigured(&self, bag_fault: BagFault) -> InviteEmailOutcome {
        let code = error_code::INVITE_EMAIL_UNAVAILABLE.as_str();
        let invite_id = self.invite.invite_id.as_str();
        let attempt = self.invite.attempt;
        let duration_ms = self.duration_ms();
        let reason = REASON_UNCONFIGURED;
        let fault = bag_fault.name();
        let field = bag_fault.field();
        tracing::error!(
            error_code = code,
            invite_id,
            attempt,
            reason,
            bag = SMTP_RELAY_BAG,
            fault,
            field,
            duration_ms,
            event = EVENT_FAILED
        );
        InviteEmailOutcome::Unconfigured
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
