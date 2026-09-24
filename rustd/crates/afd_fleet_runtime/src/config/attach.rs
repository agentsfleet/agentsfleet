//! Attaching a fleet to a chat channel: its `mention` trigger, written into
//! its `TRIGGER.md`.
//!
//! An install that names a channel lands here, so the channel lives in the
//! fleet's own document — the record `fleet update` edits and the subscriber
//! read parses — rather than in a second place fleet behaviour is configured.
//!
//! # A structural edit, not a text one
//!
//! The frontmatter goes through the converter [`crate::parse_trigger`] reads
//! it with, the trigger is added to that tree, and the tree is written back as
//! YAML. Splicing lines into the block would be a second YAML grammar, wrong on
//! the first flow-style `triggers: [...]`. The trigger itself is serialized
//! from the schema's own declaration, so what is written is what is read.
//!
//! What that costs: the stored copy's frontmatter comes back in the
//! serializer's layout, keys in their authored order but without comments. The
//! prose beneath it is kept, and the library entry it came from is untouched.

use serde_json::Value;

use super::{ChannelId, FleetConfig, Mention, TRIGGERS, Trigger, raw};
use crate::error::{Error, ErrorKind, Result, missing};
use crate::frontmatter::{FENCE, json, parse_trigger, scan};

/// The key of the namespaced block the triggers live under. The parser spells
/// it in a `serde` attribute on `raw::Document`, where a const is not accepted.
const RUNTIME_BLOCK: &str = "x-agentsfleet";

/// `document` with `mention` among its triggers.
///
/// Unchanged when the document already declares that mention. Otherwise the
/// trigger is appended; whether the result is a configuration this daemon
/// stores is the caller's parse to make, and a document already attached to a
/// DIFFERENT channel is one it refuses, because a fleet speaks to one audience.
///
/// # Errors
/// Refuses a document [`parse_trigger`] refuses.
pub fn attach_mention(document: &str, mention: &Mention) -> Result<String> {
    if parse_trigger(document)?
        .config()
        .is_attached_to(&mention.source, &mention.channel)
    {
        return Ok(document.to_owned());
    }

    let block = scan(document).ok_or(Error::from(ErrorKind::FrontmatterMissing))?;
    let mut tree = json::to_json(block.yaml())?;
    let written = serde_json::to_value(raw::Trigger::Mention {
        source: Some(mention.source.to_string()),
        channels: Some(vec![mention.channel.as_str().to_owned()]),
    })?;
    tree.get_mut(RUNTIME_BLOCK)
        .and_then(|runtime| runtime.get_mut(TRIGGERS))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| missing(TRIGGERS))?
        .push(written);

    let frontmatter = yaml_serde::to_string(&tree)?;
    Ok(match block.body() {
        "" => format!("{FENCE}\n{frontmatter}{FENCE}\n"),
        body => format!("{FENCE}\n{frontmatter}{FENCE}\n\n{body}\n"),
    })
}

impl FleetConfig {
    /// Whether this fleet answers mentions from `source` in `channel`.
    ///
    /// The one definition the install's idempotence check above and the
    /// ingress subscriber read (`afd_ingress::slack`) share, so a fleet the
    /// install calls attached is exactly one a mention is routed to. The
    /// provider compares without case because the authored `source` is
    /// free-form text.
    #[must_use]
    pub fn is_attached_to(&self, source: &str, channel: &ChannelId) -> bool {
        self.triggers().iter().any(|trigger| {
            matches!(trigger, Trigger::Mention(mention)
                if mention.source.eq_ignore_ascii_case(source) && &mention.channel == channel)
        })
    }
}

#[cfg(test)]
#[path = "attach/tests.rs"]
mod tests;
