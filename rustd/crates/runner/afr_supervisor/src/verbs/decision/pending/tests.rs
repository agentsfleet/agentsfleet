use super::PendingDecisions;
use crate::{
    client::Verb,
    halt::Halt,
    test_support::{Answer, json, plane},
};
use af_core::id::Uuid7;
use af_wire::{
    decision::{DecisionOutcome, DecisionResult, DecisionUsage},
    task::{DecisionReceipt, DecisionReceiptInput},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio_util::sync::CancellationToken;

#[tokio::test(start_paused = true)]
async fn dropping_a_receipt_caller_preserves_its_known_usage_for_lease_settlement()
-> crate::Result<()> {
    let accepting = Arc::new(AtomicBool::new(false));
    let (plane, mut calls) = receipt_peer(Arc::clone(&accepting));
    let lease = Uuid7::parse(crate::test_util::LEASE_ID)?;
    let call = Uuid7::parse("019f0000-0000-7000-8000-000000000001")?;
    let pending = PendingDecisions::default();
    let input = pending.retain(call.clone(), receipt())?;
    let halt = Halt::new(CancellationToken::new());
    let mut recording = Box::pin(pending.record(&plane, &halt, &lease, &call, &input));
    let first = tokio::select! {
        result = &mut recording => panic!("receipt must remain pending: {result:?}"),
        first = calls.recv() => first.ok_or_else(|| crate::error::config("receipt request missing"))?,
    };
    drop(recording);
    drop(input);
    tokio::time::advance(std::time::Duration::from_secs(6)).await;
    accepting.store(true, Ordering::SeqCst);
    pending.settle(&plane, &halt, &lease).await?;
    let repeated = calls
        .recv()
        .await
        .ok_or_else(|| crate::error::config("receipt retry missing"))?;
    assert_eq!(first.path, repeated.path);
    assert_eq!(first.body, repeated.body);
    let body: serde_json::Value = serde_json::from_slice(
        repeated
            .body
            .as_ref()
            .ok_or_else(|| crate::error::config("receipt body missing"))?,
    )
    .map_err(crate::error::encode)?;
    assert_eq!(
        body.pointer("/receipt/result/usage/input_tokens"),
        Some(&serde_json::json!(13))
    );
    pending.settle(&plane, &halt, &lease).await?;
    assert!(
        crate::test_support::drain(&mut calls).is_empty(),
        "acknowledgement retires the lease's receipt"
    );
    Ok(())
}

fn receipt() -> DecisionReceiptInput {
    DecisionReceiptInput {
        fencing_token: 504,
        receipt: DecisionReceipt::Provider {
            result: DecisionResult {
                reported_model: Some("fixture-model".to_owned()),
                usage: Some(DecisionUsage {
                    input_tokens: 13,
                    output_tokens: 5,
                    cached_input_tokens: 2,
                    cache_write_tokens: 0,
                }),
                result: DecisionOutcome::Refused,
            },
        },
    }
}

fn receipt_peer(
    ready: Arc<AtomicBool>,
) -> (crate::client::ControlPlane, crate::test_support::Calls) {
    plane(move |call| {
        assert_eq!(call.verb, Verb::TaskDecisionRecord);
        if !ready.load(Ordering::SeqCst) {
            return Answer::Stall;
        }
        json(
            &serde_json::json!({"call_id":"019f0000-0000-7000-8000-000000000001",
            "state":"known","replayed":false,"disposition":"stale","reserved_nanos":"0"}),
        )
    })
}
