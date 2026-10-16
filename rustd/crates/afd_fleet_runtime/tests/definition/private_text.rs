//! A refused setup never carries its author's text into a rendering.
//!
//! serde quotes the value it refuses, and a setup sent in the wrong shape is
//! private instructions. Every reader's refusal is walked whole — `Display`,
//! `Debug` and each `source()` — for a secret-looking value placed where each
//! kind of serde refusal would quote it.

use std::error::Error as _;

use afd_fleet_runtime::{Definition, FleetConfig};
use serde_json::Value;

use super::{set, value};

/// What must never appear in any rendering of a refusal.
const SECRET: &str = "sk-live-private-instructions";

fn renderings(error: &afd_fleet_runtime::Error) -> Vec<String> {
    let mut rendered = vec![error.to_string(), format!("{error:?}")];
    let mut cause = error.source();
    while let Some(link) = cause {
        rendered.push(link.to_string());
        rendered.push(format!("{link:?}"));
        cause = link.source();
    }
    rendered
}

/// The shared fixture with one member replaced, as a whole setup and as the
/// `config` document the configuration readers take.
fn replaced(pointer: &str, to: Value) -> (String, String) {
    let mut document = value();
    set(&mut document, pointer, to);
    let config = document
        .get("config")
        .expect("the shared fixture carries a config")
        .to_string();
    (document.to_string(), config)
}

#[test]
fn a_refused_setup_keeps_its_position_and_expected_type_but_never_its_text() {
    let markdown = Value::from(format!("# Instructions\n{SECRET}\n")).to_string();
    let budget = replaced("/config/x-agentsfleet/budget", Value::from(SECRET));
    let trigger = replaced(
        "/config/x-agentsfleet/triggers",
        serde_json::json!([{ "type": SECRET }]),
    );
    for (label, (setup, config), keeps) in [
        (
            "a setup sent as a JSON string",
            (markdown.clone(), markdown),
            "invalid type: string",
        ),
        ("a budget sent as a string", budget, "invalid type: string"),
        ("an unknown trigger type", trigger, "unknown variant"),
    ] {
        for (reader, outcome) in [
            ("Definition::parse", Definition::parse(&setup).map(drop)),
            (
                "FleetConfig::authored",
                FleetConfig::authored(&config).map(drop),
            ),
            (
                "FleetConfig::stored",
                FleetConfig::stored(&config).map(drop),
            ),
        ] {
            assert!(outcome.is_err(), "{reader} accepted {label}");
            let Err(refusal) = outcome else { continue };
            let rendered = renderings(&refusal);
            for text in &rendered {
                assert!(!text.contains(SECRET), "{reader}: {label} leaked: {text}");
            }
            assert!(
                rendered.iter().any(|text| text.contains(keeps)),
                "{reader}: {label} lost `{keeps}`: {rendered:?}"
            );
        }
    }
}
