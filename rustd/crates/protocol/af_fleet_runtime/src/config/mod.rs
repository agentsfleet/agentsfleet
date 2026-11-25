//! A fleet's configuration requests, each read on its own.
//!
//! What may wake it, how far its GitHub grant reaches, where its HTTP tools
//! may go, and the statements a durable task is judged against.
//!
//! # The shape of each read
//!
//! One strict serde read builds a raw type — presence, types, collections,
//! tagged unions — and names any key the schema does not read. garde proves
//! the bounds the raw type declares. One resolve then applies the rules serde
//! cannot know. Nothing walks a map, and the halves cannot interleave, because
//! they are separate types.

pub mod decision;
mod policy;
pub(crate) mod raw;
mod repository;
mod trigger;

pub use self::policy::http_egress::HttpEgress;
pub use self::repository::{
    Access, Reach, Recorded, RepositoryBinding, RepositoryPublication, ValidationWorkflow,
};
pub use self::trigger::{
    ChannelId, Cron, CronSchedule, KIND_CRON, KIND_MENTION, KIND_WEBHOOK, Mention, TRIGGER_KINDS,
    Trigger, VERIFIED_WEBHOOK_SOURCES, WEBHOOK_SOURCE_APPROVAL, WEBHOOK_SOURCE_GITHUB,
    WEBHOOK_SOURCE_LINEAR, WEBHOOK_SOURCE_SLACK, Webhook, WebhookVerifier, check_schedule,
    parse_triggers, prove_unique,
};

#[cfg(test)]
mod tests {
    use super::{RepositoryBinding, Trigger};

    #[test]
    fn a_parsed_value_is_shareable_across_threads() {
        const fn assert_shareable<T: Send + Sync>() {}
        assert_shareable::<Trigger>();
        assert_shareable::<RepositoryBinding>();
    }
}
