//! One message, built and handed to a relay, with one retry when the
//! connection rather than the relay failed.
//!
//! The retry carries the SAME message, so the same `Resend-Idempotency-Key`:
//! a relay that accepted the first copy before the connection dropped
//! recognises the second and delivers once. Resend deduplicates on that header;
//! a relay that ignores it may deliver one extra copy, which is the cost of not
//! leaving an invite unsent over a dropped socket.

use lettre::message::header::{Header, HeaderName, HeaderValue};
use lettre::message::{Mailbox, MultiPart};
use lettre::{AsyncSmtpTransport, AsyncTransport as _, Message, Tokio1Executor};

use crate::{RenderedEmail, Result};

/// The header Resend deduplicates a send on.
pub const IDEMPOTENCY_HEADER: &str = "Resend-Idempotency-Key";

/// `Resend-Idempotency-Key: invite-{invite_id}-{attempt}`.
///
/// The attempt is committed before the send, so a key names one owner action
/// and a retry inside that action reuses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IdempotencyKey(String);

impl IdempotencyKey {
    pub(crate) fn for_invite(invite_id: &str, attempt: i32) -> Self {
        Self(format!("invite-{invite_id}-{attempt}"))
    }
}

impl Header for IdempotencyKey {
    fn name() -> HeaderName {
        HeaderName::new_from_ascii_str(IDEMPOTENCY_HEADER)
    }

    fn parse(s: &str) -> core::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self(s.to_owned()))
    }

    fn display(&self) -> HeaderValue {
        HeaderValue::new(Self::name(), self.0.clone())
    }
}

/// The message: both parts, the subject, and the idempotency header.
pub(crate) fn message(
    from: Mailbox,
    to: Mailbox,
    rendered: &RenderedEmail,
    key: IdempotencyKey,
) -> Result<Message> {
    Ok(Message::builder()
        .from(from)
        .to(to)
        .subject(rendered.subject.clone())
        .header(key)
        .multipart(MultiPart::alternative_plain_html(
            rendered.text.clone(),
            rendered.html.clone(),
        ))?)
}

/// What a relay said to one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Delivery {
    /// The relay accepted it, with this reply code.
    Accepted { reply: u16 },
    /// The relay refused it — a 4xx or 5xx reply, authentication included —
    /// or TLS could not be set up; `reply` is the code when there was one.
    Refused { reply: Option<u16> },
    /// The connection failed or dropped before the relay answered.
    Unreachable,
}

/// Something that hands a message to a relay.
///
/// Production's is lettre's SMTP transport; the unit suite's is lettre's
/// `AsyncStubTransport`, which records what it was given.
pub(crate) trait Mailer: Sync {
    fn deliver(&self, message: Message) -> impl Future<Output = Delivery> + Send;
}

impl Mailer for AsyncSmtpTransport<Tokio1Executor> {
    async fn deliver(&self, message: Message) -> Delivery {
        match self.send(message).await {
            Ok(response) => Delivery::Accepted {
                reply: response.code().into(),
            },
            Err(error) => classify(&error),
        }
    }
}

/// A refusal the relay spoke, or a connection that never got that far.
///
/// A reply CODE is a refusal, and so are a client-side fault and a TLS failure:
/// trying again would meet the same answer. A missing or unparseable reply is
/// not: lettre reports a connection closed while it waited for the reply as a
/// response error ("incomplete response"), and that is exactly the dropped
/// connection the one retry exists for.
fn classify(error: &lettre::transport::smtp::Error) -> Delivery {
    let spoke = error.is_transient() || error.is_permanent() || error.is_client() || error.is_tls();
    if spoke {
        Delivery::Refused {
            reply: error.status().map(u16::from),
        }
    } else {
        Delivery::Unreachable
    }
}

/// What a send came to, and whether it took the retry to get there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Attempted {
    pub(crate) delivery: Delivery,
    pub(crate) retried: bool,
}

/// Sends `message`, and once more when the first try never reached the relay.
pub(crate) async fn send_once_retrying<M: Mailer>(mailer: &M, message: Message) -> Attempted {
    match mailer.deliver(message.clone()).await {
        Delivery::Unreachable => Attempted {
            delivery: mailer.deliver(message).await,
            retried: true,
        },
        delivery => Attempted {
            delivery,
            retried: false,
        },
    }
}

#[cfg(test)]
pub(crate) mod tests;
