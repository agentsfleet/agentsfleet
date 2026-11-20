//! `propose_setup` keeps a setup the native rules accept, against the head it
//! names, and answers every refusal with what to correct, so the run goes on.

use std::error::Error;

use af_core::money::NANOS_PER_USD;
use af_fleet_runtime::Definition;
use af_fleet_runtime::config::Trigger;
use af_fleet_runtime::setup::{DESCRIPTION, NAME, Rejection};
use guffbolt::tools::propose::Propose;
use guffbolt::{Describe as _, ErrorCode, Tool, ToolOutput};
use serde_json::{Value, json};

use super::{Base, Candidate, FleetSetup};

/// The head candidate an edit names.
const ID: &str = "01900000-0000-7000-8000-000000000001";
/// The person's message the turn answers.
const SAID: &str = "Check the support queue every morning at nine.";

/// A support setup with no triggers yet, as a definition's JSON.
fn document() -> Value {
    json!({
        "schema_version": 1,
        "description": "Checks the support queue.",
        "instructions_md": "Answer directly.",
        "config": {"name": "support-triage", "x-agentsfleet": {
            "triggers": [], "tools": [],
            "budget": {"daily_nanos": NANOS_PER_USD.to_string()}
        }}
    })
}

/// The tool for a draft whose head is that setup at `revision`.
fn head_at(revision: u64) -> Result<Propose<FleetSetup>, Box<dyn Error>> {
    let definition: Definition = serde_json::from_value(document())?;
    Ok(Propose::new(FleetSetup::new(
        Some(Candidate {
            id: ID.to_owned(),
            revision,
            definition,
        }),
        Some(SAID.to_owned()),
        false,
    )))
}

/// An edit naming the head at `base` that adds a daily cron trigger on `schedule`.
fn add_trigger(base: &str, schedule: &str) -> Value {
    json!({"proposal": {
        "kind": "edit", "base_candidate_id": ID, "base_candidate_revision": base,
        "config_ops": [{"op": "add", "path": "/config/x-agentsfleet/triggers/-", "value":
            {"type": "cron", "schedule": schedule, "timezone": "UTC", "message": "Daily check"}}]
    }})
}

/// An edit naming the head at revision 1 that changes its instructions.
fn edit_instructions(edits: &Value) -> Value {
    json!({"proposal": {
        "kind": "edit", "base_candidate_id": ID, "base_candidate_revision": "1",
        "instruction_edits": edits
    }})
}

/// One call, as a loop that lends nothing makes it.
async fn call(tool: &Propose<FleetSetup>, arguments: &Value) -> ToolOutput {
    Tool::<()>::call(tool, arguments, ()).await
}

#[tokio::test]
async fn propose_setup_keeps_a_valid_edit_against_the_head() -> Result<(), Box<dyn Error>> {
    let tool = head_at(1)?;

    let output = call(&tool, &add_trigger("1", "0 9 * * *")).await;

    assert_eq!(output.error_code, None, "{}", output.text);
    let kept = tool.kept().ok_or("the edit was not kept")?;
    assert_eq!(
        kept.base,
        Base {
            candidate_id: Some(ID.to_owned()),
            revision: 1,
        }
    );
    let [Trigger::Cron(cron)] = kept.document.runtime().triggers() else {
        return Err("the kept setup does not hold exactly the one cron trigger".into());
    };
    assert_eq!(
        (&*cron.schedule, &*cron.timezone, &*cron.message),
        ("0 9 * * *", "UTC", "Daily check")
    );
    Ok(())
}

#[tokio::test]
async fn propose_setup_returns_the_parse_failure_to_the_model() -> Result<(), Box<dyn Error>> {
    let tool = head_at(1)?;

    let refused = call(&tool, &add_trigger("1", "61 * * * *")).await;

    assert_eq!(
        refused.error_code,
        Some(ErrorCode::new("invalid_arguments"))
    );
    assert!(
        refused
            .text
            .starts_with("[invalid_arguments] No changes were applied. The setup is invalid: "),
        "{}",
        refused.text
    );
    assert!(refused.text.contains("`schedule`"), "{}", refused.text);
    assert_eq!(tool.kept(), None);
    // The run goes on: the corrected schedule, in the same run, is kept.
    let corrected = call(&tool, &add_trigger("1", "0 9 * * *")).await;
    assert_eq!(corrected.error_code, None, "{}", corrected.text);
    assert!(tool.kept().is_some());
    Ok(())
}

#[tokio::test]
async fn propose_setup_refuses_a_stale_base_revision() -> Result<(), Box<dyn Error>> {
    let tool = head_at(2)?;

    let refused = call(&tool, &add_trigger("1", "0 9 * * *")).await;

    assert_eq!(refused.error_code, Some(ErrorCode::new("stale_candidate")));
    assert_eq!(
        refused.text,
        format!(
            "[stale_candidate] No changes were applied. The current candidate is {ID} at revision 2. \
             Name that identifier and revision \"2\" as the base and return a corrected propose_setup call."
        )
    );
    assert_eq!(tool.kept(), None);
    Ok(())
}

#[tokio::test]
async fn a_later_edit_resolves_its_anchor_in_the_setup_the_run_kept() -> Result<(), Box<dyn Error>>
{
    let tool = head_at(1)?;
    call(
        &tool,
        &edit_instructions(&json!([{"kind": "append", "text": "\n\nEscalate refunds."}])),
    )
    .await;

    // The anchor exists only in the kept setup, never in the head.
    let second = call(
        &tool,
        &edit_instructions(&json!([{"kind": "replace", "old_text": "Escalate refunds.",
            "text": "Escalate refunds over fifty dollars."}])),
    )
    .await;

    assert_eq!(second.error_code, None, "{}", second.text);
    let kept = tool.kept().ok_or("nothing was kept")?;
    assert_eq!(kept.base.revision, 1);
    assert_eq!(
        kept.document.instructions_md(),
        "Answer directly.\n\nEscalate refunds over fifty dollars."
    );
    Ok(())
}

#[tokio::test]
async fn an_anchor_missing_from_the_head_is_refused_with_its_correction()
-> Result<(), Box<dyn Error>> {
    let tool = head_at(1)?;

    let refused = call(
        &tool,
        &edit_instructions(&json!([{"kind": "replace", "old_text": "Réponds vite.", "text": "x"}])),
    )
    .await;

    assert_eq!(
        refused,
        ToolOutput::failed(
            ErrorCode::new("instructions_missing"),
            Rejection::InstructionsMissing.feedback()
        )
    );
    assert_eq!(tool.kept(), None);
    Ok(())
}

#[tokio::test]
async fn a_save_refuses_every_proposal_as_no_change_requested() -> Result<(), Box<dyn Error>> {
    let definition: Definition = serde_json::from_value(document())?;
    let candidate = Candidate {
        id: ID.to_owned(),
        revision: 1,
        definition,
    };
    let tool = Propose::new(FleetSetup::new(
        Some(candidate),
        Some("Save".to_owned()),
        true,
    ));

    let refused = call(&tool, &add_trigger("1", "0 9 * * *")).await;

    assert_eq!(
        refused,
        ToolOutput::failed(
            ErrorCode::new("edit_not_requested"),
            Rejection::EditNotRequested.feedback()
        )
    );
    assert_eq!(tool.kept(), None);
    Ok(())
}

#[tokio::test]
async fn an_unknown_kind_or_a_bare_proposal_gets_the_fixed_correction() -> Result<(), Box<dyn Error>>
{
    let tool = head_at(1)?;

    for arguments in [
        json!({"proposal": {"kind": "rename", "name": "support-desk"}}),
        json!({"proposal": "edit"}),
        json!({"kind": "edit", "base_candidate_id": ID, "base_candidate_revision": "1"}),
        json!({}),
    ] {
        let refused = call(&tool, &arguments).await;
        assert_eq!(
            refused,
            ToolOutput::failed(
                ErrorCode::new("invalid_arguments"),
                Rejection::InvalidArguments.feedback()
            ),
            "{arguments}"
        );
    }

    assert_eq!(tool.kept(), None);
    Ok(())
}

#[tokio::test]
async fn a_first_setup_is_kept_with_no_base() -> Result<(), Box<dyn Error>> {
    let tool = Propose::new(FleetSetup::new(None, Some(SAID.to_owned()), false));

    let output = call(
        &tool,
        &json!({"proposal": {"kind": "initialize", "definition": document()}}),
    )
    .await;

    assert_eq!(output.error_code, None, "{}", output.text);
    let kept = tool.kept().ok_or("the first setup was not kept")?;
    assert_eq!(
        kept.base,
        Base {
            candidate_id: None,
            revision: 0,
        }
    );
    assert_eq!(kept.document.description(), "Checks the support queue.");
    Ok(())
}

#[tokio::test]
async fn an_edit_before_any_candidate_is_refused_as_stale() {
    let tool = Propose::new(FleetSetup::new(None, Some(SAID.to_owned()), false));

    let refused = call(&tool, &add_trigger("1", "0 9 * * *")).await;

    assert_eq!(
        refused,
        ToolOutput::failed(
            ErrorCode::new("stale_candidate"),
            Rejection::StaleCandidate.feedback()
        )
    );
    assert_eq!(tool.kept(), None);
}

#[test]
fn the_tool_offers_only_the_edit_form_once_a_candidate_exists() -> Result<(), Box<dyn Error>> {
    let editing = head_at(1)?;
    let first = Propose::new(FleetSetup::new(None, None, false));

    let spec = editing.spec();

    assert_eq!((spec.name, spec.description), (NAME, DESCRIPTION));
    assert_eq!(
        spec.parameters
            .pointer("/properties/proposal/properties/kind/enum"),
        Some(&json!(["edit"]))
    );
    assert_eq!(
        first
            .spec()
            .parameters
            .pointer("/properties/proposal/properties/kind/enum"),
        Some(&json!(["initialize"]))
    );
    Ok(())
}
