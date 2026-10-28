//! The published Definition schema against the parser that reads Definitions.
//!
//! Each document below goes to both, and both must answer alike: every
//! committed corpus setup is accepted by the parser AND satisfies the schema,
//! and every refusal in the table is refused by the parser AND outside the
//! schema. A schema looser than the parser tells a client a document will save
//! when it will not.

#![expect(
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

mod admits;
mod refusals;
mod verifier;

use std::path::PathBuf;

use afd_fleet_runtime::Definition;
use afd_fleet_runtime::definition::{
    ConfigOp, DefinitionSchema, InstructionEdit, MAX_DESCRIPTION_CHARS, MAX_INSTRUCTIONS_CHARS,
};
use serde_json::{Map, Value};
use utoipa::{PartialSchema, ToSchema};

use self::admits::Components;
use crate::support::{definition_path, fixture};

/// The fixture tree the corpus suites read, relative to this crate.
const CORPUS: &str = "../../../tests/fixtures/fleets";

/// Every component a Definition and an edit batch publish, as a client reads them.
fn components() -> Components {
    let mut collected = Vec::new();
    push::<DefinitionSchema>(&mut collected);
    push::<ConfigOp>(&mut collected);
    push::<InstructionEdit>(&mut collected);
    let published = collected
        .into_iter()
        .map(|(name, schema)| {
            let schema = serde_json::to_value(schema)
                .unwrap_or_else(|failure| panic!("`{name}` serializes: {failure}"));
            (name, schema)
        })
        .collect::<Map<String, Value>>();
    Components(published)
}

/// One type's own schema and every component it references.
fn push<T: ToSchema>(
    collected: &mut Vec<(
        String,
        utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
    )>,
) {
    collected.push((T::name().into_owned(), <T as PartialSchema>::schema()));
    T::schemas(collected);
}

/// The published name of the Definition component.
fn definition_component() -> String {
    DefinitionSchema::name().into_owned()
}

/// Every first-party and probe setup in the corpus, by its relative path.
fn corpus() -> Vec<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS);
    let mut found: Vec<String> = std::fs::read_dir(&root)
        .unwrap_or_else(|_missing| panic!("{} should be readable", root.display()))
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|slug| root.join(definition_path(slug)).is_file())
        .map(|slug| definition_path(&slug))
        .collect();
    found.sort();
    found
}

#[test]
fn every_corpus_setup_is_accepted_by_the_parser_and_the_schema() {
    let components = components();
    let corpus = corpus();
    assert!(!corpus.is_empty(), "the corpus holds no setup to compare");
    for relative in corpus {
        let text = fixture(&relative);
        if let Err(failure) = Definition::parse(&text) {
            panic!("the parser refuses {relative}: {failure}");
        }
        let document: Value = serde_json::from_str(&text)
            .unwrap_or_else(|failure| panic!("{relative} is JSON: {failure}"));
        assert!(
            components.admits(&definition_component(), &document),
            "the published schema refuses {relative}, which the parser accepts"
        );
    }
}

#[test]
fn the_shared_complete_fixture_is_accepted_by_both() {
    let document = super::value();
    if let Err(failure) = Definition::parse(&document.to_string()) {
        panic!("the parser refuses the shared fixture: {failure}");
    }
    assert!(components().admits(&definition_component(), &document));
}

/// A trigger set left out or left empty is a fleet only ever talked to, and
/// both the parser and the published schema accept it.
#[test]
fn an_absent_or_empty_trigger_set_is_accepted_by_both() {
    let components = components();
    for empty in [None, Some(Value::Array(Vec::new()))] {
        let mut document = super::value();
        let block = document
            .pointer_mut("/config/x-agentsfleet")
            .and_then(Value::as_object_mut)
            .unwrap_or_else(|| panic!("the shared fixture carries a runtime block"));
        match empty.clone() {
            Some(set) => block.insert("triggers".to_owned(), set),
            None => block.remove("triggers"),
        };
        if let Err(failure) = Definition::parse(&document.to_string()) {
            panic!("the parser refuses a trigger set of {empty:?}: {failure}");
        }
        assert!(
            components.admits(&definition_component(), &document),
            "the schema refuses a trigger set of {empty:?}"
        );
    }
}

/// Both texts at their character caps, in two-byte characters, are accepted by
/// both: a parser bound counted in bytes would refuse what `maxLength` admits.
#[test]
fn both_texts_at_their_character_caps_are_accepted_by_both() {
    let mut document = super::value();
    super::set(
        &mut document,
        "/description",
        serde_json::json!("é".repeat(MAX_DESCRIPTION_CHARS)),
    );
    super::set(
        &mut document,
        "/instructions_md",
        serde_json::json!("é".repeat(MAX_INSTRUCTIONS_CHARS)),
    );
    if let Err(failure) = Definition::parse(&document.to_string()) {
        panic!("the parser refuses texts at their caps: {failure}");
    }
    assert!(components().admits(&definition_component(), &document));
}
