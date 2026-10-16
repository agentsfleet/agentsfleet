//! Native setup preserves its description and every supported policy field in storage.

use afd_fleet_runtime::config::BudgetCeiling;
use afd_fleet_runtime::{Definition, FleetConfig};
use serde_json::{Value, json};

use super::{parse, set, value};

/// How a document that is not this setup's JSON renders.
const UNREADABLE: &str = "the stored configuration could not be read";

#[test]
fn test_definition_roundtrip_preserves_policy() {
    let original = parse(&value());
    let stored = serde_json::to_string(&original).expect("valid setup serializes");
    let reloaded = Definition::parse(&stored).expect("stored native setup validates");
    assert_eq!(reloaded, original);
    assert_eq!(
        serde_json::to_value(&reloaded).expect("valid JSON"),
        value()
    );
    assert_eq!(reloaded.runtime().budget().daily().nanos(), 1_000_000_001);
    assert_eq!(
        reloaded
            .runtime()
            .budget()
            .monthly()
            .map(BudgetCeiling::nanos),
        Some(10_000_000_001)
    );
    assert_eq!(reloaded.runtime().model(), Some("runtime-model"));
    assert_eq!(reloaded.description(), "Explain failures.");
}

/// The retired file format — a `---`-fenced YAML header over Markdown — is not
/// a second way in. Every reader the setup has refuses it as an unreadable
/// document, before any field is taken from the header, whether it arrives as
/// raw text, as a JSON string, or as a fence wrapped around valid JSON.
#[test]
fn a_fenced_markdown_setup_is_refused_by_every_reader() {
    let fenced = "---\nname: checkout-watch\ndescription: Explain failures.\n---\n# Instructions\n";
    let around_json = format!("---\nname: checkout-watch\n---\n{}", value());
    // What serde says about each input: a fence is not JSON at its first
    // line, and a JSON string is the wrong type for a setup object.
    for (label, input, cause_names) in [
        ("raw Markdown", fenced.to_owned(), "at line 1 column"),
        ("a fence around valid JSON", around_json, "at line 1 column"),
        (
            "Markdown as a JSON string",
            Value::from(fenced).to_string(),
            "invalid type: string",
        ),
    ] {
        for (reader, outcome) in [
            ("Definition::parse", Definition::parse(&input).map(drop)),
            (
                "FleetConfig::authored",
                FleetConfig::authored(&input).map(drop),
            ),
            ("FleetConfig::stored", FleetConfig::stored(&input).map(drop)),
        ] {
            assert!(outcome.is_err(), "{reader} accepted {label}");
            let Err(refusal) = outcome else { continue };
            assert_eq!(refusal.code().as_str(), "UZ-AGT-008", "{reader}: {label}");
            assert!(
                refusal.to_string().contains(UNREADABLE),
                "{reader} refused {label} for another reason: {refusal}"
            );
            let cause = std::error::Error::source(&refusal)
                .map(ToString::to_string)
                .unwrap_or_default();
            assert!(
                cause.contains(cause_names),
                "{reader}: {label} lost serde's cause `{cause_names}`: {cause}"
            );
        }
    }
}

#[test]
fn test_definition_roundtrip_preserves_instruction_whitespace_and_normalizes_lf() {
    let mut input = value();
    set(
        &mut input,
        "/instructions_md",
        "\n  Keep indentation.\r\n\r\nLast line.  \n".into(),
    );
    let original = parse(&input);
    assert_eq!(
        original.instructions_md(),
        "\n  Keep indentation.\n\nLast line.  \n"
    );
    let stored = serde_json::to_string(&original).expect("native setup serializes");
    assert_eq!(
        Definition::parse(&stored).expect("native setup reloads"),
        original
    );
}

#[test]
fn a_definition_read_through_serde_is_the_one_parse_returns() {
    let read: Definition = serde_json::from_value(value()).expect("serde reads the shared fixture");
    assert_eq!(read, parse(&value()));

    let mut undescribed = value();
    set(&mut undescribed, "/description", json!(""));
    let refusal = serde_json::from_value::<Definition>(undescribed)
        .expect_err("serde refuses what parse refuses");
    assert!(
        refusal.to_string().contains("`description` is required"),
        "{refusal}"
    );
}

#[test]
fn the_config_document_is_the_stored_config_and_its_policy() {
    let definition = parse(&value());
    let config = definition.config();
    let stored = serde_json::to_value(&definition).expect("a Definition serializes");
    assert_eq!(Some(config.value()), stored.get("config"));
    assert_eq!(
        config.value().pointer("/x-agentsfleet/network/allow"),
        Some(&json!(["api.github.com"]))
    );
    assert_eq!(config.runtime(), definition.runtime());
    assert_eq!(config.runtime().name().as_str(), "checkout-watch");
}
