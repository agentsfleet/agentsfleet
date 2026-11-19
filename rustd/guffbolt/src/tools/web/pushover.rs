//! `pushover`: a push notification through Pushover's one endpoint.
//!
//! The model writes the message; the handler asks the outbound hook for the
//! `pushover` secret's `token` and `user` itself and puts them in the body,
//! because Pushover takes them there rather than in a header. The model never
//! names a credential, and an argument naming one is refused by the schema.
//! The request still passes the hook, so a run whose policy lacks Pushover's
//! host cannot notify.

use std::ops::RangeInclusive;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::answered;
use crate::tool::ToolOutput;
use crate::tools::outbound::{Placement, Redirects, Request};
use crate::tools::{Context, Handler, code};

/// Pushover's message endpoint.
pub const MESSAGES_URL: &str = "https://api.pushover.net/1/messages.json";
/// The method it takes.
const METHOD: &str = "POST";
/// The body's media type.
const JSON: (&str, &str) = ("Content-Type", "application/json");
/// The secret the handler reads its credentials from.
pub const CREDENTIAL: &str = "pushover";
/// The application token's field.
pub const FIELD_TOKEN: &str = "token";
/// The recipient's field.
pub const FIELD_USER: &str = "user";
/// The priorities Pushover accepts.
const PRIORITIES: RangeInclusive<i8> = -2..=2;
/// What an out-of-range priority reads back.
const PRIORITY_OUT_OF_RANGE: &str = "priority runs from -2 to 2";

/// `pushover`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Notify {
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

/// Sends one notification through the run's outbound hook.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pushover;

impl Handler for Pushover {
    const NAME: &'static str = "pushover";
    const DESCRIPTION: &'static str = "Send a push notification through Pushover to \
        the recipient this fleet's pushover secret names.";
    type Arguments = Notify;

    async fn run(&self, arguments: Notify, context: Context<'_>) -> ToolOutput {
        if arguments
            .priority
            .is_some_and(|priority| !PRIORITIES.contains(&priority))
        {
            return ToolOutput::failed(code::INVALID_ARGUMENTS, PRIORITY_OUT_OF_RANGE);
        }
        let outbound = context.outbound;
        let token = match outbound.secret(Self::NAME, CREDENTIAL, FIELD_TOKEN).await {
            Ok(token) => token,
            Err(missing) => return missing.into(),
        };
        let user = match outbound.secret(Self::NAME, CREDENTIAL, FIELD_USER).await {
            Ok(user) => user,
            Err(missing) => return missing.into(),
        };
        let body = Body {
            token: &token,
            user: &user,
            message: &arguments.message,
            title: arguments.title.as_deref(),
            priority: arguments.priority,
            sound: arguments.sound.as_deref(),
        };
        let written = match serde_json::to_string(&body) {
            Ok(written) => written,
            Err(unwritten) => {
                return ToolOutput::failed(code::INVALID_ARGUMENTS, &unwritten.to_string());
            }
        };
        let draft = Request {
            method: METHOD.to_owned(),
            url: MESSAGES_URL.to_owned(),
            headers: vec![(JSON.0.to_owned(), JSON.1.to_owned())],
            body: Some(written),
            placement: Placement::Nowhere,
            redirects: Redirects::Stop,
        };
        match outbound.send(Self::NAME, draft).await {
            Ok(inbound) => answered(
                inbound.status,
                format!(
                    "Status: {}\n\nResponse Body:\n{}",
                    inbound.status, inbound.body
                ),
            ),
            Err(refused) => refused.into(),
        }
    }
}
