//! Pushover as the run's notifier: `guffbolt`'s `notify`, sent through
//! Pushover's one endpoint and the run's egress guard.
//!
//! The model writes the message; the backend asks the guard for the
//! `pushover` secret's `token` and `user` itself and puts them in the body,
//! because Pushover takes them there rather than in a header. The model never
//! names a credential. The request still passes the guard, so a run whose
//! policy lacks Pushover's host cannot notify.

use guffbolt::CallRef;
use guffbolt::tools::Refusal;
use guffbolt::tools::code;
use guffbolt::tools::notify::{Notification, Notifier, NotifyFuture, Priority};
use guffbolt::tools::outbound::{Outbound as _, Placement, Redirects, Request};
use serde::Serialize;

use crate::egress::Egressed;

/// Pushover's message endpoint.
pub(crate) const MESSAGES_URL: &str = "https://api.pushover.net/1/messages.json";
/// The method it takes.
const METHOD: &str = "POST";
/// The body's media type.
const JSON: (&str, &str) = ("Content-Type", "application/json");
/// The secret the backend reads its credentials from.
const CREDENTIAL: &str = "pushover";
/// The application token's field.
const FIELD_TOKEN: &str = "token";
/// The recipient's field.
const FIELD_USER: &str = "user";
/// What a send is logged as.
const TOOL: &str = "notify";
/// The statuses Pushover answers a sent notification with.
const SUCCESS: std::ops::Range<u16> = 200..300;

/// The body Pushover reads.
#[derive(Serialize)]
struct Body<'a> {
    token: &'a str,
    user: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<i8>,
}

/// Pushover's priority for `priority`: quiet, the recipient's default, or
/// past their quiet hours. `None` leaves Pushover's own default.
const fn pushover_priority(priority: Priority) -> Option<i8> {
    match priority {
        Priority::Low => Some(-1),
        Priority::Normal => None,
        Priority::High => Some(1),
    }
}

/// Sends a run's notifications through Pushover, behind the run's guard.
#[derive(Debug)]
pub(crate) struct Pushover<'b, 'run> {
    outbound: Egressed<'b, 'run>,
}

impl<'b, 'run> Pushover<'b, 'run> {
    /// The notifier sending through `outbound`, the run's guard.
    pub(crate) const fn new(outbound: Egressed<'b, 'run>) -> Self {
        Self { outbound }
    }

    /// Sends `note` and answers what the model reads of Pushover's reply.
    async fn send(&self, note: Notification<'_>) -> Result<String, Refusal> {
        let token = self.secret(FIELD_TOKEN).await?;
        let user = self.secret(FIELD_USER).await?;
        let body = Body {
            token: &token,
            user: &user,
            message: note.message,
            title: note.title,
            priority: pushover_priority(note.priority),
        };
        let written = serde_json::to_string(&body)
            .map_err(|unwritten| Refusal::new(code::INVALID_ARGUMENTS, unwritten.to_string()))?;
        let draft = Request {
            method: METHOD.to_owned(),
            url: MESSAGES_URL.to_owned(),
            headers: vec![(JSON.0.to_owned(), JSON.1.to_owned())],
            body: Some(written),
            placement: Placement::Nowhere,
            redirects: Redirects::Stop,
        };
        let inbound = self
            .outbound
            .send(TOOL, draft)
            .await
            .map_err(|refused| Refusal::new(refused.code, refused.detail))?;
        let text = format!(
            "Status: {}\n\nResponse Body:\n{}",
            inbound.status, inbound.body
        );
        if SUCCESS.contains(&inbound.status) {
            Ok(text)
        } else {
            Err(Refusal::new(code::UPSTREAM_STATUS, text))
        }
    }

    /// The `field` of the run's `pushover` secret.
    async fn secret(&self, field: &str) -> Result<String, Refusal> {
        self.outbound
            .secret(TOOL, CREDENTIAL, field)
            .await
            .map_err(|refused| Refusal::new(refused.code, refused.detail))
    }
}

impl Notifier for Pushover<'_, '_> {
    fn notify<'a>(&'a self, _call: &'a CallRef, note: Notification<'a>) -> NotifyFuture<'a> {
        Box::pin(self.send(note))
    }
}

#[cfg(test)]
mod tests;
