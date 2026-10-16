//! Which chat channel a fleet answers in: its reviewed `mention` trigger.
//!
//! The channel lives in the fleet's own Definition, the record a reviewed
//! setup revision changes, rather than in a second place fleet behaviour is
//! configured. This file only reads it.

use super::{ChannelId, FleetConfig, Trigger};

impl FleetConfig {
    /// Whether this fleet answers mentions from `source` in `channel`.
    ///
    /// The one definition the ingress subscriber reads (`afd_ingress::slack`),
    /// so a fleet a reviewed setup attaches is exactly one a mention is routed
    /// to. The provider compares without case because the authored `source` is
    /// free-form text.
    #[must_use]
    pub fn is_attached_to(&self, source: &str, channel: &ChannelId) -> bool {
        self.triggers().iter().any(|trigger| {
            matches!(trigger, Trigger::Mention(mention)
                if mention.source.eq_ignore_ascii_case(source) && &mention.channel == channel)
        })
    }
}
