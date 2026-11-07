//! Credentials stay private while canonical scrubbed tool calls survive native replay.

use afd_core::test_util::trace::Capture;
use afr_tools::catalog::UPDATE_PLAN;
use serde_json::{Value, json};
use tracing::level_filters::LevelFilter;

use super::CALL_ID;
use super::support::wires::Wire;
use super::support::{Fake, KEY, Seen, TOKEN, engine, lease, run};

// Value: protects=tool replay keeps opaque credential references and excludes original secrets;
// fails_when=replay drops or restores canonical arguments after the real loop scrubs them;
// why_new=secret absence alone does not prove the replacement tool payload survives;
// seam=none; no_claim=live provider acceptance (local HTTPS and the real agent loop).
#[tokio::test]
async fn test_api_key_never_leaves_the_supervisor() {
    for wire in Wire::ALL {
        // What the journal holds at the loudest level an operator can name.
        let capture = Capture::install_filtered(afr_providers::log_filter(LevelFilter::TRACE));
        let echoed = json!({"expression": KEY, "token": TOKEN});
        let mut fake = Fake::serve(vec![
            wire.call(CALL_ID, UPDATE_PLAN.name(), &echoed),
            wire.answer(&format!("the key was {KEY}")),
        ])
        .await;
        let leased = lease(
            &wire.provider(),
            &[UPDATE_PLAN.name()],
            &format!("use {TOKEN}"),
        );

        let (output, frames) = run(&engine(&fake), &leased).await;

        let seen = fake.seen();
        assert_eq!(seen.len(), 2, "{wire:?}: one request per turn");
        assert_requests_private(wire, &seen);
        assert_eq!(
            tool_arguments(wire, &seen[1].body),
            json!({"expression": "«secret:llm.api_key»", "token": "«secret:github.token»"}),
            "{wire:?}: replay keeps both opaque credential references"
        );
        let rendered = format!("{frames:?}{output:?}");
        assert!(
            !rendered.contains(KEY) && !rendered.contains(TOKEN),
            "{wire:?}: {rendered}"
        );
        let logged = format!("{:?}", capture.events());
        assert!(
            !logged.contains(KEY) && !logged.contains(TOKEN),
            "{wire:?}: {logged}"
        );
        assert!(
            output.result.content.contains("«secret:llm.api_key»"),
            "{wire:?}"
        );
    }
}

fn assert_requests_private(wire: Wire, seen: &[Seen]) {
    assert!(
        seen.iter()
            .all(|request| wire.carries_key(&request.headers, KEY)),
        "{wire:?}"
    );
    for request in seen {
        let mut elsewhere = request
            .headers
            .iter()
            .filter(|(name, _)| !matches!(name.as_str(), "x-api-key" | "authorization"));
        assert!(
            elsewhere.all(|(_, value)| !value.to_str().unwrap().contains(KEY)),
            "{wire:?}: the key rides one header"
        );
        let prompt = request.body.to_string();
        assert!(
            !prompt.contains(KEY) && !prompt.contains(TOKEN),
            "{wire:?}: {prompt}"
        );
    }
}

fn tool_arguments(wire: Wire, body: &Value) -> Value {
    let history = if matches!(wire, Wire::Responses) {
        &body["input"]
    } else {
        &body["messages"]
    };
    let entries = history.as_array().unwrap();
    match wire {
        Wire::Messages => entries
            .iter()
            .find(|item| item["role"] == "assistant")
            .unwrap()["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|part| part["type"] == "tool_use" && part["id"] == CALL_ID)
            .unwrap()["input"]
            .clone(),
        Wire::Responses => {
            let call = entries
                .iter()
                .find(|item| item["type"] == "function_call" && item["call_id"] == CALL_ID)
                .unwrap();
            serde_json::from_str(call["arguments"].as_str().unwrap()).unwrap()
        }
        Wire::Chat => {
            let assistant = entries
                .iter()
                .find(|item| item["role"] == "assistant")
                .unwrap();
            let call = assistant["tool_calls"]
                .as_array()
                .unwrap()
                .iter()
                .find(|call| call["id"] == CALL_ID)
                .unwrap();
            serde_json::from_str(call["function"]["arguments"].as_str().unwrap()).unwrap()
        }
    }
}
