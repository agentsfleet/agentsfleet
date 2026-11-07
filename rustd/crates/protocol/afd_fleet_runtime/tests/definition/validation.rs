//! Failure boundaries reject ambiguous setup and preserve the input on failure.

use afd_fleet_runtime::definition::{
    MAX_DEFINITION_BYTES, MAX_DESCRIPTION_CHARS, MAX_INITIAL_INSTRUCTIONS_CHARS,
    MAX_INSTRUCTIONS_CHARS,
};
use afd_fleet_runtime::{Definition, FleetConfig};
use serde_json::{Value, json};

use super::{parse, set, value};

/// How a refused bound renders.
const OUT_OF_BOUNDS: &str = "outside its bounds";

/// How a value of the wrong JSON type renders.
const UNREADABLE: &str = "could not be read";

/// How an absent or empty description renders.
const DESCRIPTION_REQUIRED: &str = "`description` is required";

/// How a key outside `config`'s own members renders.
const UNSUPPORTED_CONFIG_KEY: &str = "unsupported configuration key";

/// Each refusal names its own cause, so a refusal that starts failing for a
/// different reason (a bound lifted, a type check dropped) fails here.
#[test]
fn test_definition_rejects_unsupported_versions_descriptions_and_names() {
    for (pointer, replacement, cause) in [
        ("/schema_version", json!(2), OUT_OF_BOUNDS),
        ("/schema_version", json!("1"), UNREADABLE),
        (
            "/config/name",
            json!("Checkout Watch"),
            "is not a fleet name",
        ),
        ("/description", json!(""), DESCRIPTION_REQUIRED),
        ("/description", json!(42), UNREADABLE),
        ("/description", json!(null), UNREADABLE),
        (
            "/description",
            json!("one\0line"),
            "must not contain a NUL character",
        ),
        (
            "/description",
            json!("a".repeat(MAX_DESCRIPTION_CHARS + 1)),
            OUT_OF_BOUNDS,
        ),
    ] {
        let mut input = value();
        *input.pointer_mut(pointer).expect("existing field") = replacement;
        let refusal = Definition::parse(&input.to_string()).expect_err("a refused field");
        assert!(
            chain(&refusal).contains(cause),
            "{pointer}: expected `{cause}`, got {}",
            chain(&refusal)
        );
    }
    for (pointer, field, cause) in [
        ("", "grant", "unknown field `grant`"),
        ("/config", "secret", UNSUPPORTED_CONFIG_KEY),
        (
            "/config/x-agentsfleet",
            "model_key",
            "`model_key` is not a known `x-agentsfleet` key",
        ),
        (
            "/config/x-agentsfleet/network",
            "secret",
            UNSUPPORTED_CONFIG_KEY,
        ),
        (
            "/config/x-agentsfleet/gates/rules/0",
            "grant",
            UNSUPPORTED_CONFIG_KEY,
        ),
    ] {
        let mut input = value();
        input.pointer_mut(pointer).expect("existing object")[field] = json!("forged");
        let refusal = Definition::parse(&input.to_string()).expect_err("an unknown key");
        assert!(
            chain(&refusal).contains(cause),
            "{pointer}/{field}: expected `{cause}`, got {}",
            chain(&refusal)
        );
    }
}

#[test]
fn test_definition_empty_editing_is_valid_but_save_is_blocked() {
    let mut input = value();
    set(&mut input, "/instructions_md", json!(""));
    parse(&input)
        .validate_for_save()
        .expect_err("empty instructions cannot be saved");
    parse(&value())
        .validate_for_save()
        .expect("the complete fixture can be saved");
}

/// Two-byte characters, so the allowance holds only if it counts characters.
#[test]
fn a_first_generation_may_not_exceed_its_smaller_instruction_allowance() {
    let with_instructions = |chars: usize| {
        let mut input = value();
        set(&mut input, "/instructions_md", json!("é".repeat(chars)));
        parse(&input)
    };
    with_instructions(MAX_INITIAL_INSTRUCTIONS_CHARS)
        .validate_initial_generation()
        .expect("instructions at the allowance are a valid first generation");
    let over = with_instructions(MAX_INITIAL_INSTRUCTIONS_CHARS + 1);
    let refusal = over
        .validate_initial_generation()
        .expect_err("one character over the allowance is refused");
    assert!(refusal.to_string().contains(OUT_OF_BOUNDS), "{refusal}");
    over.validate_for_save()
        .expect("the same instructions are within the bound a later edit keeps");
}

/// Instructions are bounded in characters: a cap of two-byte characters is
/// twice the cap in bytes, so reverting to a byte bound refuses it.
#[test]
fn test_definition_text_limits_are_characters_and_the_envelope_is_bytes() {
    let mut input = value();
    set(
        &mut input,
        "/instructions_md",
        json!("é".repeat(MAX_INSTRUCTIONS_CHARS)),
    );
    Definition::parse(&input.to_string()).expect("instructions at the character limit parse");
    set(
        &mut input,
        "/instructions_md",
        json!("é".repeat(MAX_INSTRUCTIONS_CHARS + 1)),
    );
    let refusal =
        Definition::parse(&input.to_string()).expect_err("one character over the limit is refused");
    assert!(refusal.to_string().contains(OUT_OF_BOUNDS), "{refusal}");
    set(&mut input, "/instructions_md", json!("private\0text"));
    Definition::parse(&input.to_string()).expect_err("a NUL in instructions is refused");
    let refusal = Definition::parse(&" ".repeat(MAX_DEFINITION_BYTES + 1))
        .expect_err("an envelope over the byte limit is refused");
    assert!(refusal.to_string().contains(OUT_OF_BOUNDS), "{refusal}");
}

/// The envelope never refuses what the text bounds admit: both texts at their
/// caps, every character a six-byte JSON escape, still parse.
#[test]
fn the_envelope_admits_both_texts_at_their_caps_in_the_widest_encoding() {
    let widest = '\u{1}';
    let mut input = value();
    set(
        &mut input,
        "/description",
        json!(widest.to_string().repeat(MAX_DESCRIPTION_CHARS)),
    );
    set(
        &mut input,
        "/instructions_md",
        json!(widest.to_string().repeat(MAX_INSTRUCTIONS_CHARS)),
    );
    Definition::parse(&input.to_string()).expect("both texts at their caps parse");
}

#[test]
fn test_definition_stored_and_authored_repository_write_rules_match() {
    let mut input = value()
        .get("config")
        .cloned()
        .expect("the fixture carries a config");
    input
        .get_mut("x-agentsfleet")
        .and_then(Value::as_object_mut)
        .expect("runtime block")
        .remove("repository_base");
    for result in [
        FleetConfig::stored(&input.to_string()),
        FleetConfig::authored(&input.to_string()),
    ] {
        assert!(result.is_err(), "write without approved base was accepted");
    }
}

/// Every message in `error`'s chain, outermost first, so a refusal can be
/// matched on the cause serde named beneath this crate's own sentence.
fn chain(error: &afd_fleet_runtime::Error) -> String {
    let mut rendered = error.to_string();
    let mut cause = std::error::Error::source(error);
    while let Some(found) = cause {
        rendered.push_str(": ");
        rendered.push_str(&found.to_string());
        cause = found.source();
    }
    rendered
}

/// The description is required and bounded in characters: a cap of two-byte
/// characters is twice the cap in bytes, so reverting to a byte bound refuses it.
#[test]
fn a_description_is_required_and_bounded_in_characters() {
    let mut input = value();
    object_at(&mut input, "").remove("description");
    let refusal = Definition::parse(&input.to_string()).expect_err("no description");
    assert!(
        refusal.to_string().contains(DESCRIPTION_REQUIRED),
        "{refusal}"
    );

    let mut input = value();
    set(
        &mut input,
        "/description",
        json!("é".repeat(MAX_DESCRIPTION_CHARS)),
    );
    Definition::parse(&input.to_string()).expect("a description at the character bound parses");
    set(
        &mut input,
        "/description",
        json!("é".repeat(MAX_DESCRIPTION_CHARS + 1)),
    );
    let refusal =
        Definition::parse(&input.to_string()).expect_err("one character over the bound is refused");
    assert!(refusal.to_string().contains(OUT_OF_BOUNDS), "{refusal}");
}

/// A description keeps its line breaks, stored with one line ending.
#[test]
fn a_description_keeps_line_breaks_with_normalized_endings() {
    let mut input = value();
    set(
        &mut input,
        "/description",
        json!("Watch checkout.\r\nExplain failures.\r"),
    );
    assert_eq!(
        parse(&input).description(),
        "Watch checkout.\nExplain failures.\n"
    );
}

/// The object at `pointer` in `document`, which the shared fixture carries.
fn object_at<'a>(document: &'a mut Value, pointer: &str) -> &'a mut serde_json::Map<String, Value> {
    document
        .pointer_mut(pointer)
        .and_then(Value::as_object_mut)
        .expect("an object the fixture carries")
}

/// The retired `skill` member is an unknown key like any other: refused by
/// name, never read for a name or a description.
#[test]
fn a_skill_member_is_refused_by_name() {
    let mut input = value();
    object_at(&mut input, "").insert(
        "skill".into(),
        json!({"name": "checkout-watch", "description": "Explain failures.", "version": "1.0.0"}),
    );
    let refusal = Definition::parse(&input.to_string()).expect_err("a skill member");
    assert!(
        chain(&refusal).contains("unknown field `skill`"),
        "{}",
        chain(&refusal)
    );
}

/// Runner `tags` are not setup: no level of the Definition accepts them, and
/// each level refuses them as an unknown key rather than for some other reason.
#[test]
fn tags_cannot_be_set_anywhere() {
    for (pointer, cause) in [
        ("", "unknown field `tags`"),
        ("/config", UNSUPPORTED_CONFIG_KEY),
        (
            "/config/x-agentsfleet",
            "`tags` is not a known `x-agentsfleet` key",
        ),
    ] {
        let mut input = value();
        object_at(&mut input, pointer).insert("tags".into(), json!(["gpu"]));
        let refusal = Definition::parse(&input.to_string()).expect_err("tags");
        assert!(
            chain(&refusal).contains(cause),
            "tags at `{pointer}`: expected `{cause}`, got {}",
            chain(&refusal)
        );
    }
}
