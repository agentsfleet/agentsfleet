//! Reasoning is private metadata; an unexpected executable call cannot become notes.
use super::{SUMMARY, WAIT, checkpoint, json, model_calls, ready};
use afd_core::error_code;
use afd_wire::context::ContextState;

// Value: native reasoning can accompany valid notes without escaping to the runner.
#[tokio::test]
#[ignore = "requires live Postgres and Dragonfly: make test-integration-rustd"]
async fn summary_reasoning_is_private_and_a_tool_call_with_valid_notes_is_refused() {
    for tool in [false, true] {
        let (f, attempt, mut live) = ready().await;
        let task = live.call(&f.runner, &attempt, model_calls::fixture::request(&attempt));
        live.arrived().await;
        assert_eq!(
            live.context().await.progress.state,
            ContextState::Summarizing
        );
        live.text(SUMMARY).await;
        let response = response(tool);
        live.frame(json!({"type":"response.completed","response":response}))
            .await;
        assert_eq!(
            live.context().await.progress.state,
            if tool {
                ContextState::Interrupted
            } else {
                ContextState::Resuming
            }
        );
        let result = tokio::time::timeout(WAIT, task)
            .await
            .expect("known usage settles")
            .expect("dispatch task");
        if tool {
            assert_eq!(
                result.expect_err("tools are not working notes").code(),
                error_code::AUTHORING_PROVIDER
            );
            assert!(checkpoint(&f, &attempt).await.is_none());
        } else {
            let result = result.expect("usable notes");
            assert!(result.compacted && result.text.is_empty() && result.proposal_calls.is_empty());
            assert!(checkpoint(&f, &attempt).await.is_some());
        }
        assert_eq!(
            model_calls::fixture::receipts(&f, &attempt).await,
            (1, 1, false)
        );
        live.drained().await;
        drop(live.reader);
        f.cleanup().await;
    }
}

fn response(tool: bool) -> serde_json::Value {
    let extra = if tool {
        json!({"type":"function_call","id":"fc_untrusted","call_id":"call_untrusted",
            "name":"propose_setup","arguments":"{}"})
    } else {
        json!({"type":"reasoning","id":"rs_private","summary":[],
            "encrypted_content":"opaque-private-reasoning"})
    };
    json!({"status":"completed","output":[extra,
        {"type":"message","content":[{"type":"output_text","text":SUMMARY}]}],
        "usage":{"input_tokens":100,"output_tokens":1}})
}
