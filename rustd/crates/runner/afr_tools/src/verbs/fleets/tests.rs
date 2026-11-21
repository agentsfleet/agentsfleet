//! `fleet_activity`: the list or one fleet's events, asked as the model
//! named them.

use serde_json::json;

use super::FleetActivity;
use crate::handler::Typed;
use crate::runtime::ToolErrorCode;
use crate::testing::{Asked, OwnedFleetsCall, RecordingVerbs, call, run_with};
use guffbolt::ToolOutput;

const FLEET: &str = "0190d5c2-7e1a-7000-8000-00000000f0f3";

async fn read(arguments: serde_json::Value) -> (ToolOutput, Vec<Asked>) {
    let verbs = RecordingVerbs::answering(Ok(r#"{"items":[]}"#.to_owned()), Ok(true));
    let run = run_with(&verbs);
    let output = call(&*Typed::boxed(FleetActivity), &run, arguments).await;
    (output, verbs.asked())
}

/// No fleet named: the workspace's list, the body handed back as it came.
#[tokio::test]
async fn fleet_activity_lists_the_workspace_without_a_fleet_id() {
    let (output, asked) = read(json!({"limit": 5})).await;
    assert_eq!(output.text, r#"{"items":[]}"#);
    assert_eq!(
        asked,
        [Asked::Fleets(OwnedFleetsCall::List {
            limit: Some(5),
            starting_after: None,
        })]
    );
}

/// A fleet named: its events, with the paging passed through.
#[tokio::test]
async fn fleet_activity_reads_one_fleets_events() {
    let (_output, asked) = read(json!({"fleet_id": FLEET, "starting_after": "c1"})).await;
    assert_eq!(
        asked,
        [Asked::Fleets(OwnedFleetsCall::Events {
            fleet: FLEET.to_owned(),
            limit: None,
            starting_after: Some("c1".to_owned()),
        })]
    );
}

/// A fleet id that is not one is refused before any call.
#[tokio::test]
async fn fleet_activity_refuses_a_fleet_id_that_is_not_one() {
    let (output, asked) = read(json!({"fleet_id": "docs-bot"})).await;
    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::InvalidArguments.into())
    );
    assert!(asked.is_empty(), "{asked:?}");
}
