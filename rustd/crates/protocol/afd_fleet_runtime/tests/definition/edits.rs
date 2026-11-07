//! Partial or stale proposal payloads cannot partially replace a valid Definition.

#![expect(
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_fleet_runtime::Definition;
use afd_fleet_runtime::definition::{
    ConfigOp, EditBatch, InstructionEdit, MAX_INSTRUCTION_OFFSET, MAX_INSTRUCTIONS_CHARS,
};
use serde_json::{Value, json};

use super::{parse, set, value};

#[test]
fn test_definition_atomic_edit_preserves_unedited_policy_and_array_order() {
    let definition = parse(&value());
    let changed = definition
        .edit(EditBatch {
            config_ops: vec![ConfigOp::Add {
                path: "/config/x-agentsfleet/tools/1".into(),
                value: json!("read_logs"),
            }],
            instruction_edits: vec![InstructionEdit {
                start_byte: 0,
                end_byte: 11,
                text: "Inspect".into(),
            }],
        })
        .expect("valid small edit");
    assert_eq!(
        changed
            .runtime()
            .tools()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>(),
        ["http_request", "read_logs", "shell"]
    );
    assert!(changed.instructions_md().starts_with("Inspect failures."));
    assert_eq!(changed.runtime().budget(), definition.runtime().budget());
    assert_eq!(definition, parse(&value()), "base is immutable");
}

#[test]
fn test_definition_bad_batch_never_changes_base() {
    let original = parse(&value());
    for path in [
        "/schema_version",
        "/config",
        "/grants/0",
        "/skill/name",
        "/tags",
        "/description",
        "/config/x-agentsfleet/tags",
        "/config/x-agentsfleet/tools/+0",
        "/config/x-agentsfleet/tools/01",
        "/config/x-agentsfleet/tools/999999999999999999999999999",
        "/config/x-agentsfleet/network/not~2valid",
    ] {
        let batch = EditBatch {
            config_ops: vec![
                ConfigOp::Replace {
                    path: "/description".into(),
                    value: json!("changed"),
                },
                ConfigOp::Add {
                    path: path.into(),
                    value: json!("forged"),
                },
            ],
            ..EditBatch::default()
        };
        assert!(original.edit(batch).is_err(), "accepted {path}");
        assert_eq!(original, parse(&value()));
    }
}

#[test]
fn test_definition_utf8_overlap_and_invalid_end_are_rejected() {
    let mut input = value();
    set(&mut input, "/instructions_md", json!("évidence"));
    let original = parse(&input);
    for ranges in [
        vec![(1, 2)],
        vec![(0, 99)],
        vec![(3, 2)],
        vec![(0, 3), (2, 4)],
        vec![(0, 0), (0, 0)],
    ] {
        let batch = EditBatch {
            instruction_edits: ranges
                .into_iter()
                .map(|(start_byte, end_byte)| InstructionEdit {
                    start_byte,
                    end_byte,
                    text: "replacement".into(),
                })
                .collect(),
            ..EditBatch::default()
        };
        original
            .edit(batch)
            .expect_err("an overlapping or out-of-range edit is refused");
        assert_eq!(original, parse(&input));
    }
}

#[test]
fn test_definition_replacement_budget_counts_all_operations_before_overwrites() {
    let original = parse(&value());
    let batch = EditBatch {
        config_ops: ["a".repeat(16_384), "b".repeat(16_384), "valid".into()]
            .into_iter()
            .map(|text| ConfigOp::Replace {
                path: "/description".into(),
                value: json!(text),
            })
            .collect(),
        ..EditBatch::default()
    };
    original
        .edit(batch)
        .expect_err("replacements over the budget are refused before any applies");
    assert_eq!(original, parse(&value()));
}

/// How a refused replacement bound renders.
const OUT_OF_BOUNDS: &str = "outside its bounds";

/// The fixture's tools, as the replacement characters an array value counts.
const TOOLS: [&str; 2] = ["http_request", "shell"];

/// A two-byte character, so a budget that counted bytes would refuse at half
/// the bound these tests reach.
const TWO_BYTE: &str = "é";

/// `operation`, with instruction text that brings the batch to `total`
/// replacement characters once `counted` characters of the operation are added.
///
/// The text replaces the whole base, so the edited instructions are the text
/// alone and stay inside their own bound: only the replacement bound can refuse.
fn batch(definition: &Definition, operation: ConfigOp, counted: usize, total: usize) -> EditBatch {
    EditBatch {
        config_ops: vec![operation],
        instruction_edits: vec![InstructionEdit {
            start_byte: 0,
            end_byte: definition.instructions_md().len(),
            text: TWO_BYTE.repeat(total - counted),
        }],
    }
}

#[test]
fn an_edit_with_no_operation_is_refused_and_changes_nothing() {
    let original = parse(&value());
    let refusal = original
        .edit(EditBatch::default())
        .expect_err("an empty batch is refused");
    assert!(
        refusal
            .to_string()
            .contains("an edit must contain at least one operation"),
        "{refusal}"
    );
    assert_eq!(original, parse(&value()));
}

#[test]
fn an_array_value_counts_the_characters_of_every_string_it_holds() {
    let original = parse(&value());
    let tools = || ConfigOp::Replace {
        path: "/config/x-agentsfleet/tools".into(),
        value: json!(TOOLS),
    };
    let counted = TOOLS.iter().map(|tool| tool.chars().count()).sum();

    let at_bound = original
        .edit(batch(&original, tools(), counted, MAX_INSTRUCTIONS_CHARS))
        .expect("a batch at the bound is kept");
    assert_eq!(
        at_bound.instructions_md().chars().count(),
        MAX_INSTRUCTIONS_CHARS - counted
    );

    let refusal = original
        .edit(batch(
            &original,
            tools(),
            counted,
            MAX_INSTRUCTIONS_CHARS + 1,
        ))
        .expect_err("one character over the bound is refused");
    assert!(refusal.to_string().contains(OUT_OF_BOUNDS), "{refusal}");
}

#[test]
fn an_object_value_counts_its_string_values_and_not_its_keys() {
    let original = parse(&value());
    let (host, post_path) = ("api.github.com", "/graphql");
    let network = || ConfigOp::Replace {
        path: "/config/x-agentsfleet/network".into(),
        value: json!({"allow": [host], "read_only": true, "read_post_paths": [post_path]}),
    };
    let counted = host.chars().count() + post_path.chars().count();

    let at_bound = original
        .edit(batch(&original, network(), counted, MAX_INSTRUCTIONS_CHARS))
        .expect("a batch at the bound is kept");
    let kept = at_bound
        .runtime()
        .network()
        .expect("the replaced network is kept");
    assert_eq!(kept.allow(), [Box::from(host)]);

    let refusal = original
        .edit(batch(
            &original,
            network(),
            counted,
            MAX_INSTRUCTIONS_CHARS + 1,
        ))
        .expect_err("one character over the bound is refused");
    assert!(refusal.to_string().contains(OUT_OF_BOUNDS), "{refusal}");
}

#[test]
fn a_scalar_value_and_a_removal_count_no_replacement_characters() {
    let original = parse(&value());
    let free = [
        (
            ConfigOp::Replace {
                path: "/config/x-agentsfleet/gates/timeout_ms".into(),
                value: json!(120_000),
            },
            "/config/x-agentsfleet/gates/timeout_ms",
            json!(120_000),
        ),
        (
            ConfigOp::Replace {
                path: "/config/x-agentsfleet/network/read_only".into(),
                value: json!(false),
            },
            "/config/x-agentsfleet/network/read_only",
            json!(false),
        ),
        (
            ConfigOp::Remove {
                path: "/config/x-agentsfleet/model".into(),
            },
            "/config/x-agentsfleet/model",
            Value::Null,
        ),
    ];
    for (operation, pointer, expected) in free {
        let changed = original
            .edit(batch(&original, operation, 0, MAX_INSTRUCTIONS_CHARS))
            .unwrap_or_else(|failure| {
                panic!("{pointer} counts characters it should not: {failure}")
            });
        let stored = serde_json::to_value(&changed).expect("a Definition serializes");
        assert_eq!(
            stored.pointer(pointer).cloned().unwrap_or(Value::Null),
            expected,
            "{pointer} was not applied"
        );
        assert_eq!(
            changed.instructions_md().chars().count(),
            MAX_INSTRUCTIONS_CHARS
        );
    }
}

/// The description is a leaf a batch replaces whole: replacing it is an edit,
/// while adding, removing, emptying or reaching below it changes nothing.
#[test]
fn the_description_is_replaced_whole_and_nothing_else() {
    let original = parse(&value());
    let replace = |text: &str| EditBatch {
        config_ops: vec![ConfigOp::Replace {
            path: "/description".into(),
            value: json!(text),
        }],
        ..EditBatch::default()
    };
    let changed = original
        .edit(replace("Explain checkout failures with evidence."))
        .expect("a replaced description is kept");
    assert_eq!(
        changed.description(),
        "Explain checkout failures with evidence."
    );
    assert_eq!(changed.runtime(), original.runtime(), "policy is untouched");

    let refused = [
        ConfigOp::Add {
            path: "/description".into(),
            value: json!("added"),
        },
        ConfigOp::Remove {
            path: "/description".into(),
        },
        ConfigOp::Replace {
            path: "/description/0".into(),
            value: json!("below"),
        },
    ];
    for operation in refused {
        let batch = EditBatch {
            config_ops: vec![operation.clone()],
            ..EditBatch::default()
        };
        let refusal = original
            .edit(batch)
            .expect_err("only a whole replacement reaches the description");
        assert!(
            refusal
                .to_string()
                .contains("the description only by replace"),
            "{operation:?}: {refusal}"
        );
    }
    let emptied = original
        .edit(replace(""))
        .expect_err("an empty description is a missing one");
    assert!(
        emptied.to_string().contains("`description` is required"),
        "{emptied}"
    );
    assert_eq!(original, parse(&value()), "base is immutable");
}

/// How a path outside config and the description renders.
const NOT_WRITABLE: &str = "only config descendants and the description are writable";

/// How a key outside `config`'s own members renders.
const UNSUPPORTED_CONFIG_KEY: &str = "unsupported configuration key";

/// Neither the retired `skill` member nor runner `tags` can be written by an
/// edit, at the top or anywhere below config, and each is refused for the
/// reason its level gives rather than some other failure.
#[test]
fn an_edit_cannot_write_a_skill_member_or_tags() {
    let original = parse(&value());
    for (path, cause) in [
        ("/skill", NOT_WRITABLE),
        ("/tags", NOT_WRITABLE),
        ("/config/tags", UNSUPPORTED_CONFIG_KEY),
        ("/config/skill", UNSUPPORTED_CONFIG_KEY),
        (
            "/config/x-agentsfleet/tags",
            "`tags` is not a known `x-agentsfleet` key",
        ),
        (
            "/config/x-agentsfleet/skill",
            "`skill` is not a known `x-agentsfleet` key",
        ),
    ] {
        let batch = EditBatch {
            config_ops: vec![ConfigOp::Add {
                path: path.into(),
                value: json!(["gpu"]),
            }],
            ..EditBatch::default()
        };
        let refusal = original.edit(batch).expect_err("a refused path");
        assert!(
            refusal.to_string().contains(cause),
            "{path}: expected `{cause}`, got {refusal}"
        );
    }
    assert_eq!(original, parse(&value()));
}

/// Offsets address the stored UTF-8 bytes while text bounds count characters:
/// a two-byte character spans two offsets, and instructions at their character
/// cap are edited at an offset past that cap.
#[test]
fn instruction_offsets_stay_bytes_while_text_bounds_count_characters() {
    let mut input = value();
    set(&mut input, "/instructions_md", json!("évidence"));
    let changed = parse(&input)
        .edit(EditBatch {
            instruction_edits: vec![InstructionEdit {
                start_byte: 0,
                end_byte: TWO_BYTE.len(),
                text: "E".into(),
            }],
            ..EditBatch::default()
        })
        .expect("a whole two-byte character is replaced");
    assert_eq!(changed.instructions_md(), "Evidence");

    set(
        &mut input,
        "/instructions_md",
        json!(TWO_BYTE.repeat(MAX_INSTRUCTIONS_CHARS)),
    );
    let at_cap = parse(&input);
    let end = at_cap.instructions_md().len();
    assert!(end > MAX_INSTRUCTIONS_CHARS && end <= MAX_INSTRUCTION_OFFSET);
    let changed = at_cap
        .edit(EditBatch {
            instruction_edits: vec![InstructionEdit {
                start_byte: end - TWO_BYTE.len(),
                end_byte: end,
                text: "e".into(),
            }],
            ..EditBatch::default()
        })
        .expect("an offset past the character cap reaches the last character");
    assert!(changed.instructions_md().ends_with("ée"));
}
