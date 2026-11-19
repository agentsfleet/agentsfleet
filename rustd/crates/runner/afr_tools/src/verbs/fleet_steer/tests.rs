//! `fleet_steer`: the work handed over, a repeat named, or the refusal read
//! with its code.

use std::borrow::Cow;

use af_wire::steer_verb::FleetSteerAccepted;
use serde_json::json;

use super::{DETAIL_FLEET_ID, FleetSteer};
use crate::handler::Typed;
use crate::runtime::{ToolErrorCode, ToolOutput};
use crate::testing::{Asked, RecordingVerbs, STEERED_EVENT, call, lease_with};
use crate::verbs::Unanswered;

const TARGET: &str = "0190d5c2-7e1a-7000-8000-00000000f0f2";

/// Steers `arguments` through verbs answering `steered`.
async fn steered(
    steered: Result<FleetSteerAccepted<'static>, Unanswered>,
    arguments: serde_json::Value,
) -> (ToolOutput, Vec<Asked>) {
    let verbs = RecordingVerbs::answering(Ok(String::new()), Ok(true)).steering(steered);
    let lease = lease_with(&verbs);
    let output = call(Typed::boxed(FleetSteer).as_ref(), &lease, arguments).await;
    (output, verbs.asked())
}

fn accepted(replayed: bool) -> FleetSteerAccepted<'static> {
    FleetSteerAccepted {
        event_id: Cow::Borrowed(STEERED_EVENT),
        replayed,
    }
}

/// The work reaches the named fleet with the words as sent, and the model is
/// told the event and that the other fleet runs on its own.
#[tokio::test]
async fn fleet_steer_hands_the_words_to_the_named_fleet() {
    let (output, asked) = steered(
        Ok(accepted(false)),
        json!({"fleet_id": TARGET, "text": "run the hourly check"}),
    )
    .await;
    assert_eq!(output.error_code, None);
    assert!(output.text.contains(STEERED_EVENT), "{}", output.text);
    assert!(output.text.contains("on its own"), "{}", output.text);
    assert_eq!(
        asked,
        [Asked::Steer {
            fleet: TARGET.to_owned(),
            text: "run the hourly check".to_owned(),
        }]
    );
}

/// A repeat is named as the same run, so the model does not count two.
#[tokio::test]
async fn a_replayed_steer_reads_as_the_same_run() {
    let (output, _asked) = steered(
        Ok(accepted(true)),
        json!({"fleet_id": TARGET, "text": "run the hourly check"}),
    )
    .await;
    assert!(output.text.contains("same run"), "{}", output.text);
}

/// An id that is not one never leaves the runner.
#[tokio::test]
async fn a_fleet_id_that_is_not_one_is_refused_here() {
    let (output, asked) = steered(
        Ok(accepted(false)),
        json!({"fleet_id": "docs-bot", "text": "hello"}),
    )
    .await;
    assert_eq!(output.error_code, Some(ToolErrorCode::InvalidArguments));
    assert!(output.text.ends_with(DETAIL_FLEET_ID), "{}", output.text);
    assert!(asked.is_empty(), "{asked:?}");
}

/// A refusal reaches the model with its registry code and hint.
#[tokio::test]
async fn a_refused_steer_names_its_code() {
    let (output, _asked) = steered(
        Err(Unanswered::Refused(Some(
            af_core::error_code::FLEET_STEER_SELF,
        ))),
        json!({"fleet_id": TARGET, "text": "hello"}),
    )
    .await;
    assert_eq!(output.error_code, Some(ToolErrorCode::AgentsfleetdRefused));
    assert!(output.text.contains("UZ-AGT-018"), "{}", output.text);
}
