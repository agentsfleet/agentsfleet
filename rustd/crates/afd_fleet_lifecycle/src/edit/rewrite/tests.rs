#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]
use super::{Rewrite, Snapshot};
use crate::edit::{ConfigSource, Patch};
use crate::{Error, FleetStatus};

/// A `TRIGGER.md` naming `probe`, the smallest one that parses.
const TRIGGER: &str = "---\nname: probe\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  budget:\n    daily_dollars: 1.0\n---\n";

/// A `SKILL.md` naming `probe`.
const SKILL: &str = "---\nname: probe\ndescription: A probe.\nversion: 1.0.0\n---\nProse.\n";

/// A `SKILL.md` naming something else.
const SKILL_RENAMED: &str =
    "---\nname: other\ndescription: A probe.\nversion: 1.0.0\n---\nProse.\n";

/// A row named `name` whose documents both name `probe`.
fn stored(name: &str) -> Snapshot {
    Snapshot {
        name: name.to_owned(),
        status: FleetStatus::Active,
        source_markdown: SKILL.to_owned(),
        trigger_markdown: Some(TRIGGER.to_owned()),
    }
}

/// `document`, its `probe` name line rewritten to `name`.
fn renamed(document: &str, name: &str) -> String {
    document.replace("name: probe", &format!("name: {name}"))
}

/// A row whose name and documents all say `name`.
fn stored_as(name: &str) -> Snapshot {
    Snapshot {
        name: name.to_owned(),
        status: FleetStatus::Active,
        source_markdown: renamed(SKILL, name),
        trigger_markdown: Some(renamed(TRIGGER, name)),
    }
}

fn read(request: &Patch, current: &Snapshot) -> Result<Rewrite, Error> {
    Rewrite::read(request, current)
}

#[test]
fn a_changed_name_line_renames_the_fleet() {
    let request = Patch {
        config: Some(ConfigSource::Trigger(TRIGGER.to_owned())),
        ..Patch::default()
    };
    let rewrite = read(&request, &stored_as("was-called-this")).expect("a usable document");

    assert_eq!(rewrite.name.as_deref(), Some("probe"));
    assert!(rewrite.config_json.is_some(), "the document declares both");
    assert_eq!(rewrite.trigger_markdown.as_deref(), Some(TRIGGER));
}

#[test]
fn an_edit_keeping_the_name_line_keeps_the_installed_name() {
    // `probe-b` was named at install apart from its documents' `probe`;
    // giving it a tool must not collapse it back onto `probe`.
    let with_a_tool = TRIGGER.replace("tools: []", "tools:\n    - shell");
    let request = Patch {
        config: Some(ConfigSource::Trigger(with_a_tool.clone())),
        ..Patch::default()
    };
    let rewrite = read(&request, &stored("probe-b")).expect("a usable document");

    assert_eq!(rewrite.name, None, "the row keeps probe-b");
    assert_eq!(
        rewrite.trigger_markdown.as_deref(),
        Some(with_a_tool.as_str())
    );
    assert!(
        rewrite.config_json.is_some(),
        "the configuration is rewritten"
    );
}

#[test]
fn a_suffixed_fleet_takes_back_its_own_skill() {
    // Its documents name `probe`, so a `SKILL.md` naming `probe` is its
    // own, not a different fleet's.
    let request = Patch {
        config: Some(ConfigSource::Trigger(TRIGGER.to_owned())),
        source_markdown: Some(SKILL.to_owned()),
        ..Patch::default()
    };
    let rewrite = read(&request, &stored("probe-b")).expect("the documents agree");

    assert_eq!(rewrite.name, None);
    assert_eq!(rewrite.source_markdown.as_deref(), Some(SKILL));
}

#[test]
fn a_suffixed_fleet_still_refuses_a_skill_naming_another() {
    let request = Patch {
        source_markdown: Some(SKILL_RENAMED.to_owned()),
        ..Patch::default()
    };
    let failure = read(&request, &stored("probe-b")).expect_err("the names disagree");

    assert_eq!(
        failure.code(),
        afd_core::error_code::AGENTSFLEET_NAME_MISMATCH
    );
}

#[test]
fn a_row_without_a_stored_trigger_compares_against_its_own_name() {
    let mut current = stored("probe");
    current.trigger_markdown = None;
    let request = Patch {
        config: Some(ConfigSource::Trigger(TRIGGER.to_owned())),
        ..Patch::default()
    };
    let rewrite = read(&request, &current).expect("a usable document");

    assert_eq!(rewrite.name, None, "probe stays probe");
}

#[test]
fn a_configuration_sent_directly_never_renames_the_fleet() {
    // Inferring a name from a JSON blob would rename a fleet on a request
    // that said nothing about its name.
    let request = Patch {
        config: Some(ConfigSource::Json("{\"name\":\"other\"}".to_owned())),
        ..Patch::default()
    };
    let rewrite = read(&request, &stored("probe")).expect("no reparse to fail");

    assert_eq!(rewrite.name, None);
    assert_eq!(rewrite.config_json.as_deref(), Some("{\"name\":\"other\"}"));
    assert_eq!(rewrite.trigger_markdown, None, "the column is untouched");
}

#[test]
fn a_replacement_skill_must_agree_with_the_documents_name() {
    let request = Patch {
        source_markdown: Some(SKILL_RENAMED.to_owned()),
        ..Patch::default()
    };
    let failure = read(&request, &stored("probe")).expect_err("the names disagree");

    assert_eq!(
        failure.code(),
        afd_core::error_code::AGENTSFLEET_NAME_MISMATCH
    );
}

#[test]
fn both_documents_may_be_renamed_in_one_request() {
    // The replacement `SKILL.md` is checked against the name the TRIGGER
    // will write, not against what is stored — otherwise a rename would
    // need two requests and be inconsistent between them.
    let request = Patch {
        config: Some(ConfigSource::Trigger(TRIGGER.to_owned())),
        source_markdown: Some(SKILL.to_owned()),
        ..Patch::default()
    };
    let rewrite = read(&request, &stored_as("the-old-name")).expect("one coherent rename");

    assert_eq!(rewrite.name.as_deref(), Some("probe"));
    assert!(rewrite.required_tags.is_some(), "tags are re-derived");
}

#[test]
fn a_status_only_patch_leaves_every_document_column_alone() {
    let request = Patch {
        status: Some(crate::Requested::Stopped),
        ..Patch::default()
    };
    let current = stored("probe");
    let rewrite = read(&request, &current).expect("nothing to reparse");

    assert_eq!(rewrite.config_json, None);
    assert_eq!(rewrite.source_markdown, None);
    assert_eq!(rewrite.name, None);
    // And the post-update tag is the stored one, so an open editor is not
    // 412'd by somebody else stopping the fleet.
    assert_eq!(rewrite.source_after(&current), SKILL);
    assert_eq!(rewrite.trigger_after(&current), Some(TRIGGER));
}
