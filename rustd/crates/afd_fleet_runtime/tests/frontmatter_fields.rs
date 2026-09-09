//! The corpus's field VALUES, read against the Rust parser.
//!
//! Sibling of `frontmatter_corpus.rs`, which asserts which documents open and
//! which are refused. This one takes the documents that DO open and checks that
//! every authored value survives the conversion — a document that parses into
//! the wrong policy is a parity break the verdict table cannot see.
//!
//! # Mapping
//!
//! | Zig test (`frontmatter_fixtures_test.zig`) | Rust test here |
//! |---|---|
//! | `trigger/minimal.md` field values | [`the_minimal_trigger_carries_its_authored_values`] |
//! | `trigger/full.md` field values | [`the_full_trigger_carries_every_authored_block`] |
//! | `with_model_and_context.md`, `tool_window: auto` | [`an_auto_tool_window_resolves_to_the_zero_sentinel`] |
//! | `skill/full.md` optional fields | [`the_full_skill_carries_every_optional_field`] |
//! | "first-party library fixtures use the supported HTTP request tool" | [`every_first_party_bundle_agrees_with_its_directory_name`] |
//! | "declarative schedule has no local cron tool" | [`the_declarative_schedule_bundle_carries_its_cron_fields`] |
//! | `skill/name_mismatch/` cross-file identity | [`the_name_mismatch_pair_parses_and_disagrees`] |

#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_fleet_runtime::{parse_skill, parse_trigger};

use crate::support::{FIRST_PARTY, MODEL_VALUE, TOOL_HTTP_REQUEST, fixture};

/// `trigger/minimal.md` carries the cron trigger, tool and ceiling it declares.
#[test]
fn the_minimal_trigger_carries_its_authored_values() {
    let parsed = parse_trigger(&fixture("trigger/minimal.md")).expect("a usable document");
    let config = parsed.config();

    assert_eq!(config.name().as_str(), "minimal-skill");
    assert_eq!(config.tools().len(), 1);
    assert_eq!(&*config.tools()[0], "agentmail");
    assert!((config.budget().daily().dollars() - 1.0).abs() < f64::EPSILON);
    assert!(matches!(
        config.triggers(),
        [afd_fleet_runtime::config::Trigger::Cron(cron)] if &*cron.schedule == "0 9 * * *"
    ));
}

/// `trigger/full.md` carries its webhook signature, credentials and both ceilings.
#[test]
fn the_full_trigger_carries_every_authored_block() {
    let parsed = parse_trigger(&fixture("trigger/full.md")).expect("a usable document");
    let config = parsed.config();

    assert_eq!(config.name().as_str(), "full-skill");
    assert_eq!(config.tools().len(), 3);
    assert_eq!(config.credentials().len(), 2);
    assert!(config.budget().monthly().is_some());
    let network = config.network().expect("an allow list");
    assert_eq!(network.allow().len(), 2);

    let [afd_fleet_runtime::config::Trigger::Webhook(hook)] = config.triggers() else {
        panic!("full.md declares exactly one webhook trigger");
    };
    assert_eq!(&*hook.source, "github");
    let signature = hook.signature.as_ref().expect("a signature block");
    assert_eq!(signature.header(), "x-hub-signature-256");
    assert_eq!(signature.prefix(), "sha256=");
    assert_eq!(signature.secret_ref(), "github_secret");
}

/// `tool_window: auto` resolves to the runner's zero sentinel, not to a refusal.
///
/// The one place the corpus exercises a knob that takes a WORD where its
/// siblings take a number, and the answer is the Zig's: zero means auto.
#[test]
fn an_auto_tool_window_resolves_to_the_zero_sentinel() {
    let parsed =
        parse_trigger(&fixture("trigger/with_model_and_context.md")).expect("a usable document");
    let config = parsed.config();
    let context = config.context().expect("a context block");

    assert_eq!(config.model(), Some(MODEL_VALUE));
    assert_eq!(context.context_cap_tokens, 256_000);
    assert_eq!(context.tool_window, 0);
    assert_eq!(context.memory_checkpoint_every, 5);
    assert!((context.stage_chunk_threshold - 0.75).abs() < f32::EPSILON);
}

/// `skill/full.md` carries every optional authoring field.
#[test]
fn the_full_skill_carries_every_optional_field() {
    let skill = parse_skill(&fixture("skill/full.md")).expect("a usable document");

    assert_eq!(skill.name().as_str(), "full-skill");
    assert_eq!(skill.version().as_str(), "1.2.3");
    assert_eq!(skill.author(), Some("agentsfleet"));
    assert_eq!(skill.model(), Some("claude-sonnet-4-6"));
    assert!(skill.when_to_use().is_some());
    assert_eq!(skill.tags().len(), 3);
}

/// Every first-party bundle's two documents agree with each other and with the
/// directory that holds them, and each declares the one supported tool.
#[test]
fn every_first_party_bundle_agrees_with_its_directory_name() {
    for slug in FIRST_PARTY {
        let skill = parse_skill(&fixture(&format!("{slug}/SKILL.md")))
            .unwrap_or_else(|failure| panic!("{slug}/SKILL.md should parse: {failure}"));
        let parsed = parse_trigger(&fixture(&format!("{slug}/TRIGGER.md")))
            .unwrap_or_else(|failure| panic!("{slug}/TRIGGER.md should parse: {failure}"));

        assert_eq!(skill.name().as_str(), slug, "{slug} SKILL name");
        assert_eq!(parsed.config().name().as_str(), slug, "{slug} TRIGGER name");

        let tools = parsed.config().tools();
        assert_eq!(tools.len(), 1, "{slug} declares one tool");
        assert_eq!(&*tools[0], TOOL_HTTP_REQUEST, "{slug} tool");
    }
}

/// The declarative-schedule bundle carries its cron fields and no local timer.
#[test]
fn the_declarative_schedule_bundle_carries_its_cron_fields() {
    let parsed = parse_trigger(&fixture("zoho-sprint-daily-summarizer/TRIGGER.md"))
        .expect("a usable document");

    let [afd_fleet_runtime::config::Trigger::Cron(cron)] = parsed.config().triggers() else {
        panic!("the summarizer declares exactly one cron trigger");
    };
    assert_eq!(&*cron.schedule, "0 9 * * *");
    assert_eq!(&*cron.timezone, "Asia/Kolkata");
    assert_eq!(
        &*cron.message, "Summarize today's Zoho Sprints activity",
        "the scheduled run's stated purpose"
    );
}

/// The mismatch pair parses on both sides and disagrees about the name.
///
/// The disagreement is the point: neither parser refuses it, so the cross-file
/// identity check belongs to the install handler and this fixture is what
/// proves the check has something to catch.
#[test]
fn the_name_mismatch_pair_parses_and_disagrees() {
    let skill = parse_skill(&fixture("skill/name_mismatch/SKILL.md")).expect("a usable document");
    let parsed =
        parse_trigger(&fixture("skill/name_mismatch/TRIGGER.md")).expect("a usable document");

    assert_ne!(
        skill.name().as_str(),
        parsed.config().name().as_str(),
        "the fixture exists to disagree"
    );
}

/// A fleet may not author the daemon's own gate kind.
///
/// The wiring proof, and it is a separate claim from the validator's own unit
/// test: `not_daemon_owned` refusing a string proves the function, not that
/// `parse` ever reaches it. `gate_kind` sits inside `Option<Vec<GateRule>>` and
/// is only reached because `Gates::rules` carries `#[garde(dive)]`, so a lost
/// `dive` would leave the guard present and inert — the shape this repository has
/// been bitten by before (a producer graded as declared, never as called).
///
/// The escalation being refused: the kind travels verbatim onto the approval
/// card, and `afd_approval`'s resolve moves a credential grant for cards of this
/// kind. A fleet that could spell it would get an operator to hand it standing
/// permission to mint a third party's credentials while answering about a tool.
#[test]
fn a_fleet_may_not_author_a_daemon_owned_gate_kind() {
    for reserved in ["integration_grant", "repository_write"] {
        let document = trigger_declaring_gate_kind(reserved);
        let refused = parse_trigger(&document);
        assert!(
            refused.is_err(),
            "{reserved} must be refused through parse, not merely by the validator"
        );
    }
}

/// The same document with an ordinary kind still parses.
///
/// Without this the test above would also pass if the document were malformed
/// for some unrelated reason, which would make the guard look wired when it was
/// not.
#[test]
fn an_ordinary_gate_kind_still_parses_in_the_same_document() {
    let document = trigger_declaring_gate_kind("destructive_action");
    let parsed = parse_trigger(&document).expect("an ordinary kind is a fleet's own business");
    assert_eq!(parsed.config().name().as_str(), "gate-kind-probe");
}

/// One trigger document whose single gate rule declares `kind`.
fn trigger_declaring_gate_kind(kind: &str) -> String {
    format!(
        "---\nname: gate-kind-probe\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  \
         budget:\n    daily_dollars: 1.0\n  gates:\n    rules:\n      - tool: shell\n        \
         action: run\n        gate_kind: {kind}\n---\n"
    )
}
