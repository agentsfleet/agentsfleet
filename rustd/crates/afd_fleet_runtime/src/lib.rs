//! Fleet configuration: the stored authoring document, parsed once into the
//! typed policy every gate reads.
//!
//! # What this crate is for
//!
//! A fleet's behaviour is authored as a document — YAML frontmatter in
//! `TRIGGER.md`, stored as JSON in `core.fleets.config_json`. At claim time the
//! daemon has to turn that document into decisions: what may wake this fleet,
//! what it may spend, which of its actions need a human, how far its
//! credentials reach. This crate is that transformation, and nothing else. It
//! opens no socket, touches no datastore, and holds no clock.
//!
//! # How it differs from the daemon it replaces
//!
//! `fleet_runtime/config*.zig` is about nine hundred non-test lines across
//! seven modules, most of which is shape work — `obj.get(key) orelse return
//! error`, a `switch` per value, an `alloc.dupe` per string, and an `errdefer`
//! behind each to unwind a partially-built struct. Every one of those is
//! something serde does from a type declaration, and ownership does at scope
//! exit.
//!
//! What is left is the half that is actually about fleets, and it now reads as
//! that: a name is a kebab slug, a ceiling is positive and finite, two webhook
//! triggers may not share a source, a repository binding is optional as a WHOLE
//! rather than key by key.
//!
//! Five behaviours changed on purpose, and each is declared in [`error`] or in
//! [`frontmatter`]: a malformed field no longer reports as a missing one, a
//! non-string `skill` is no longer silently dropped, a gate failure keeps its
//! own class, a threshold is bounded by a constant named for what it bounds,
//! and the same no-silent-drop rule now covers every optional `SKILL.md` key.
//!
//! # Two documents, one scan
//!
//! [`frontmatter`] owns the `---` fence walk and the YAML-to-JSON conversion
//! that install and config-PATCH need; [`instructions`] takes the prose half
//! for the lease path. Both go through one scanner, so the two daemons cannot
//! disagree about where a document's configuration ends.
//!
//! # What it deliberately does not do
//!
//! It does not verify a webhook signature and it does not call a provider. A
//! [`provider::WebhookProvider`] answers three pieces of metadata so an
//! authored signature block can be completed; the clients that USE that
//! metadata live at the sites that make the call. `webhook_verify.zig` fuses
//! the two, which is why a config parse there drags the verification path in
//! behind it.

#![deny(unused_crate_dependencies)]

pub mod config;
pub mod error;
pub mod frontmatter;
pub mod instructions;
pub mod name;
pub mod provider;

pub use self::config::FleetConfig;
pub use self::error::{Error, Result};
pub use self::frontmatter::{ParsedTrigger, SkillMetadata, parse_skill, parse_trigger};
pub use self::instructions::instructions;
pub use self::name::{CredentialName, FleetName, Version};
pub use self::provider::{ProviderRegistry, WebhookProvider};
