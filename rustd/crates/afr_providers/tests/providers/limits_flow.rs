//! Operating thresholds compact tool data using the physical model headroom.

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

// Value: protects=compaction preserves tools after crossing the operating threshold;
// fails_when=physical headroom is incorrectly clamped to the fleet operating cap;
// why_new=existing provider bounds do not drive a tools-disabled summary and continuation;
// seam=local HTTPS provider and real executor socket;
// no_claim=live provider or sandbox acceptance.
#[tokio::test]
async fn a_tool_result_past_the_operating_cap_is_compacted_before_tools_continue() {
    let (_scratch, client, _text) = workspace().await;
    let wire = Wire::Messages;
    let notes = json!({"goal":"Read context.txt, then answer.","constraints":[],"decisions":[],
        "completed":["Read context.txt"],"remaining":["answer"],"references":["context.txt"]})
    .to_string();
    let mut fake = Fake::serve(vec![
        tool_reply(wire),
        wire.answer(&notes),
        wire.answer("Final answer"),
    ])
    .await;
    let mut leased = lease(
        &wire.provider(),
        &[FILE_READ.name()],
        "Read context.txt, then answer.",
    );
    leased.policy.inference = Some(afd_wire::inference::ModelInference {
        api: afd_wire::inference::InferenceApi::Messages,
        base_url: format!("{}/v1", fake.base),
        dialect: None,
        max_output_tokens: 4096,
    });
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
    assert_requests(&mut fake, wire);
}

fn assert_requests(fake: &mut Fake, wire: Wire) {
    let seen = fake.seen();
    assert_eq!(
        seen.len(),
        3,
        "tool turn, silent summary, then normal continuation"
    );
    assert_eq!(wire.offered(&seen[0].body), [FILE_READ.name()]);
    assert!(
        wire.offered(&seen[1].body).is_empty(),
        "summary has no tools"
    );
    assert_eq!(seen[1].body["max_tokens"], 2048);
    assert!(seen[1].body.to_string().contains("bounded tool result"));
    assert_eq!(wire.offered(&seen[2].body), [FILE_READ.name()]);
    assert!(
        seen[2]
            .body
            .to_string()
            .contains("Read context.txt, then answer.")
    );
    assert!(!seen[2].body.to_string().contains("bounded tool result"));
    assert!(seen[1].raw.len() > OPERATING as usize);
    for request in &seen {
        let output_cap = request.body["max_tokens"].as_u64().unwrap();
        assert!(output_cap > 0 && output_cap <= 4096);
        assert!(output_cap + request.raw.len() as u64 <= u64::from(PHYSICAL));
    }
}
