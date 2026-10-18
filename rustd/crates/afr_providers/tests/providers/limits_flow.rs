//! Operating thresholds stop tools without spending the model's physical window.

use std::sync::Arc;
use std::time::Duration;

use afr_agent::ResultOutcome;
use afr_egress::testing::RecordingTransport;
use afr_tools::{Catalog, catalog::FILE_READ};
use serde_json::json;

use super::support::{Fake, Reply, engine_hosting, lease, run_with, wires::Wire};

const OPERATING: u32 = 8192;
const PHYSICAL: u32 = 32_768;
const FILE: &str = "context.txt";

async fn workspace() -> (tempfile::TempDir, afr_executor::Client, String) {
    let scratch = tempfile::tempdir().unwrap();
    let socket = scratch.path().join("executor.sock");
    let root = scratch.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    let text = "bounded tool result\n".repeat(700);
    std::fs::write(root.join(FILE), &text).unwrap();
    tokio::spawn(async move { afr_executor::serve(&socket, &root).await });
    let client = afr_executor::Client::connect_within(
        &scratch.path().join("executor.sock"),
        Duration::from_secs(20),
    )
    .await
    .unwrap();
    (scratch, client, text)
}

fn tool_reply(wire: Wire) -> Reply {
    let Reply::Stream(mut events) =
        wire.call("read-context", FILE_READ.name(), &json!({"path": FILE}))
    else {
        panic!("the tool reply streams");
    };
    let start = events
        .iter_mut()
        .find(|event| event.contains("event: message_start"))
        .unwrap();
    assert!(start.contains("\"input_tokens\":6"));
    *start = start.replace("\"input_tokens\":6", "\"input_tokens\":8192");
    Reply::Stream(events)
}

// Value: protects=the final answer after a tool crosses the operating threshold;
// fails_when=physical headroom is incorrectly clamped to the fleet operating cap;
// why_new=existing provider bounds do not drive the agent's stop-tools transition;
// seam=local HTTPS provider and real executor socket;
// no_claim=live provider or sandbox acceptance.
#[tokio::test]
async fn a_tool_result_past_the_operating_cap_still_allows_the_final_answer() {
    let (_scratch, client, text) = workspace().await;
    let wire = Wire::Messages;
    let mut fake = Fake::serve(vec![tool_reply(wire), wire.answer("Final answer")]).await;
    let mut leased = lease(
        &wire.provider(),
        &[FILE_READ.name()],
        "Read context.txt, then answer.",
    );
    leased.policy.model_context_tokens = PHYSICAL;
    leased.policy.context.context_cap_tokens = OPERATING;
    let (transport, _sent) = RecordingTransport::replying(200, "");
    let engine = engine_hosting(&fake, Catalog::hosted(Arc::new(transport)));

    let (output, _frames) = run_with(&engine, &leased, Some(&client)).await;

    assert!(
        matches!(output.result.outcome, ResultOutcome::Completed(_)),
        "{:?}",
        output.result.outcome
    );
    assert_eq!(output.result.content, "Final answer");
    let seen = fake.seen();
    assert_eq!(seen.len(), 2, "one provider request per turn");
    assert!(seen[0].raw.len() < OPERATING as usize);
    assert_eq!(wire.offered(&seen[0].body), [FILE_READ.name()]);
    assert!(
        wire.offered(&seen[1].body).is_empty(),
        "the operating threshold removes tools"
    );
    // rig renders historical tool results as text once no tools are offered.
    assert_eq!(
        seen[1].body["messages"][2]["content"][0]["text"]
            .as_str()
            .unwrap()
            .strip_prefix("[tool file_read result] "),
        Some(text.as_str())
    );
    assert!(seen[1].raw.len() > OPERATING as usize);
    let output_cap = seen[1].body["max_tokens"].as_u64().unwrap();
    assert!(output_cap > 0 && output_cap <= 4096, "saved output ceiling");
    assert!(output_cap + seen[1].raw.len() as u64 <= u64::from(PHYSICAL));
}
