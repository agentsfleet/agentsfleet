//! `pushover`: a push notification through Pushover's one endpoint.
//!
//! The model writes the message; the handler reads the `pushover` secret's
//! `token` and `user` from `secrets_map` itself and puts them in the body,
//! because Pushover takes them there rather than in a header. The model never
//! names a credential, and an argument naming one is refused by the schema.
//! The request still passes the lease's guard, so a fleet whose allowlist
//! lacks Pushover's host cannot notify.

use std::ops::RangeInclusive;

use afr_egress::{Draft, Placement, Refusal};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::catalog::{Entry, PUSHOVER};
use crate::egress::{self, SharedTransport};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

/// Pushover's message endpoint.
const MESSAGES_URL: &str = "https://api.pushover.net/1/messages.json";
/// The method it takes.
const METHOD: &str = "POST";
/// The body's media type.
const JSON: (&str, &str) = ("Content-Type", "application/json");
/// The secret the handler reads its credentials from.
const CREDENTIAL: &str = "pushover";
/// The application token's field.
const FIELD_TOKEN: &str = "token";
/// The recipient's field.
const FIELD_USER: &str = "user";
/// The priorities Pushover accepts.
const PRIORITIES: RangeInclusive<i8> = -2..=2;
/// What an out-of-range priority reads back.
const PRIORITY_OUT_OF_RANGE: &str = "priority runs from -2 to 2";

/// `pushover`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Notify {
    /// The notification's text.
    message: String,
    /// A title above it.
    title: Option<String>,
    /// From -2 (no alert) to 2 (emergency); 0 when absent.
    #[schemars(range(min = -2, max = 2))]
    priority: Option<i8>,
    /// One of Pushover's sound names.
    sound: Option<String>,
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    sound: Option<&'a str>,
}

/// Sends one notification through the lease's guard.
#[derive(Debug)]
pub(crate) struct Pushover {
    transport: SharedTransport,
}

impl Pushover {
    /// A handler sending through `transport`.
    pub(crate) fn new(transport: SharedTransport) -> Self {
        Self { transport }
    }
}

#[async_trait::async_trait]
impl Handler for Pushover {
    const ENTRY: &'static Entry = &PUSHOVER;
    const DESCRIPTION: &'static str = "Send a push notification through Pushover to \
        the recipient this fleet's pushover secret names.";
    type Arguments = Notify;

    async fn run(&self, arguments: Notify, context: ToolContext<'_, '_>) -> ToolOutput {
        if arguments
            .priority
            .is_some_and(|priority| !PRIORITIES.contains(&priority))
        {
            return ToolOutput::failed(ToolErrorCode::InvalidArguments, PRIORITY_OUT_OF_RANGE);
        }
        let statics = context.lease.egress.statics();
        let credential = |field: &str| {
            statics
                .field(CREDENTIAL, field)
                .ok_or_else(|| Refusal::secret_not_found(CREDENTIAL, field))
        };
        let (token, user) = match (credential(FIELD_TOKEN), credential(FIELD_USER)) {
            (Ok(token), Ok(user)) => (token, user),
            (Err(missing), _) | (_, Err(missing)) => return egress::refused(Self::ENTRY, &missing),
        };
        let body = Body {
            token,
            user,
            message: &arguments.message,
            title: arguments.title.as_deref(),
            priority: arguments.priority,
            sound: arguments.sound.as_deref(),
        };
        let written = match serde_json::to_string(&body) {
            Ok(written) => written,
            Err(unwritten) => {
                return ToolOutput::failed(ToolErrorCode::InvalidArguments, &unwritten.to_string());
            }
        };
        let draft = Draft {
            method: METHOD.to_owned(),
            url: MESSAGES_URL.to_owned(),
            headers: vec![(JSON.0.to_owned(), JSON.1.to_owned())],
            body: Some(written),
            placement: Placement::Nowhere,
        };
        match egress::send(Self::ENTRY, self.transport.as_ref(), context.lease, draft).await {
            Ok(inbound) => egress::answered(
                inbound.status,
                format!(
                    "Status: {}\n\nResponse Body:\n{}",
                    inbound.status, inbound.body
                ),
            ),
            Err(refused) => refused,
        }
    }
}

#[cfg(test)]
#[path = "pushover/tests.rs"]
mod tests;
