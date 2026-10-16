//! Persisted output ceilings share the context window with the complete prompt.

use afd_wire::inference::{InferenceApi, ModelInference};
use afr_providers::{Connect as _, Connector, Message, Provider, Request};
use futures_util::StreamExt as _;

use super::support::{Fake, lease, wires::Wire};

const CONTEXT: u32 = 32_768;

fn provider(fake: &Fake, wire: Wire) -> Box<dyn Provider> {
    let mut lease = lease(&wire.provider(), &[], "hello");
    lease.policy.context.context_cap_tokens = CONTEXT;
    lease.policy.inference = Some(ModelInference {
        api: match wire {
            Wire::Chat => InferenceApi::Chat,
            Wire::Messages => InferenceApi::Messages,
            Wire::Responses => InferenceApi::Responses,
        },
        base_url: format!("{}/v1", fake.base),
        dialect: None,
        max_output_tokens: CONTEXT,
    });
    Connector::with_client(fake.client.clone())
        .connect(&lease)
        .unwrap()
}

fn request(messages: &[Message]) -> Request<'_> {
    Request {
        model: "workspace/configured-model",
        instructions: "Use the complete conversation.",
        messages,
        tools: &[],
        hosted: &[],
        cache_key: "fleet-context-proof",
    }
}

#[tokio::test]
async fn output_shares_the_saved_window_with_the_first_and_growing_prompts() {
    for wire in Wire::ALL {
        let mut fake = Fake::serve(vec![wire.answer("one"), wire.answer("two")]).await;
        let provider = provider(&fake, wire);
        let mut messages = vec![Message::User("hello".into())];
        for _ in 0..2 {
            let chunks: Vec<_> = provider.stream(request(&messages)).collect().await;
            assert!(chunks.iter().all(Result::is_ok), "{wire:?}: {chunks:?}");
            messages.push(Message::User("more context ".repeat(100)));
        }
        let seen = fake.seen();
        assert_eq!(seen.len(), 2);
        let limits: Vec<_> = seen
            .iter()
            .map(|seen| {
                let body = &seen.body;
                let cap = body["max_output_tokens"]
                    .as_u64()
                    .or_else(|| body["max_tokens"].as_u64())
                    .or_else(|| body["max_completion_tokens"].as_u64())
                    .unwrap();
                assert!(cap > 0 && cap + seen.raw.len() as u64 <= u64::from(CONTEXT));
                cap
            })
            .collect();
        assert!(limits[1] < limits[0], "{wire:?}: {limits:?}");
    }
}

#[tokio::test]
async fn a_prompt_that_exhausts_context_never_reaches_the_provider() {
    for wire in Wire::ALL {
        let mut fake = Fake::serve(vec![wire.answer("unused")]).await;
        let provider = provider(&fake, wire);
        let messages = [Message::User("x".repeat(CONTEXT as usize))];
        let chunks: Vec<_> = provider.stream(request(&messages)).collect().await;
        let [Err(failure)] = chunks.as_slice() else {
            panic!("{wire:?}: {chunks:?}")
        };
        assert!(failure.detail().contains("inference settings"));
        assert_eq!(failure.failure_class(), None);
        assert!(fake.seen().is_empty());
        assert_eq!(fake.connections(), 0);
    }
}
