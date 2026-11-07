//! Native provider streams are mocks on real local sockets, with no paid calls.

use afd_wire::inference::{InferenceApi, ModelInference};
use afr_providers::{Chunk, Connect as _, Connector, Message, Request, ToolSpec};
use futures_util::StreamExt as _;
use serde_json::{Value, json};

use super::support::{Fake, KEY, Reply, lease};

const GOOGLE: &str = "gemini";
const OLLAMA: &str = "ollama";
const MODEL: &str = "configured-model";
const TOOL: &str = "inspect_build";
const QUESTION: &str = "Inspect this build.";
const ANSWER: &str = "Build inspected.";
const SIGNATURE: &str = "c2lnbmVkLWdlbWluaS10aG91Z2h0";
const CACHE_KEY: &str = "conversation-key";

/// The output limit every native fixture saves.
const MAX_OUTPUT_TOKENS: u32 = 4096;

/// A connector reaching `fake`, with the model's saved settings choosing `api`.
struct Native {
    inner: Connector,
    settings: ModelInference,
}

fn connector(fake: &Fake, api: InferenceApi) -> Native {
    Native {
        inner: Connector::with_client(fake.client.clone()),
        settings: ModelInference {
            api,
            base_url: fake.base.clone(),
            dialect: None,
            max_output_tokens: MAX_OUTPUT_TOKENS,
        },
    }
}

fn gemini(parts: &Value) -> Reply {
    let event = json!({"candidates":[{"content":{"role":"model","parts":parts},
        "finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":10,
        "cachedContentTokenCount":2,"candidatesTokenCount":3,"thoughtsTokenCount":1}});
    Reply::Stream(vec![format!("data: {event}\n\n")])
}

fn ollama(message: &Value) -> Reply {
    let start = json!({"model":MODEL,"message":message,"done":false});
    let end = json!({"model":MODEL,"message":{"role":"assistant","content":""},
        "done":true,"done_reason":"stop","prompt_eval_count":10,"eval_count":4});
    Reply::Stream(vec![format!("{start}\n"), format!("{end}\n")])
}

async fn streamed(connector: &Native, provider: &str, messages: &[Message]) -> Vec<Chunk> {
    let mut leased = lease(provider, &[], QUESTION);
    leased.policy.inference = Some(connector.settings.clone());
    let model = connector.inner.connect(&leased).unwrap();
    let parameters = json!({"type":"object","properties":{}});
    let tools = [ToolSpec {
        name: TOOL,
        description: QUESTION,
        parameters: &parameters,
    }];
    let request = Request {
        model: MODEL,
        instructions: QUESTION,
        messages,
        tools: &tools,
        hosted: &[],
        cache_key: CACHE_KEY,
        max_output_tokens: None,
        single_attempt: false,
    };
    model.stream(request).map(Result::unwrap).collect().await
}

fn continuation(chunks: Vec<Chunk>) -> Vec<Message> {
    let calls = chunks
        .iter()
        .filter_map(|chunk| match chunk {
            Chunk::Call(call) => Some(call.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 1);
    let id = calls[0].id.clone();
    let replay = chunks
        .into_iter()
        .find_map(|chunk| match chunk {
            Chunk::End(end) => Some(end.replay),
            _ => None,
        })
        .unwrap();
    vec![
        Message::User(QUESTION.to_owned()),
        Message::Assistant {
            text: String::new(),
            calls,
            replay,
        },
        Message::ToolResult {
            call_id: id,
            output: ANSWER.to_owned(),
            image: None,
        },
    ]
}

#[tokio::test]
async fn test_native_gemini_continues_tool_calls() {
    let mut fake = Fake::serve(vec![
        gemini(&json!([{"functionCall":{"name":TOOL,"args":{}},"thoughtSignature":SIGNATURE}])),
        gemini(&json!([{"text":ANSWER}])),
    ])
    .await;
    let connector = connector(&fake, InferenceApi::Gemini);
    let chunks = streamed(&connector, GOOGLE, &[Message::User(QUESTION.to_owned())]).await;
    let messages = continuation(chunks);
    streamed(&connector, GOOGLE, &messages).await;
    let seen = fake.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(
        seen[1].body["contents"][1]["parts"][0]["thoughtSignature"],
        SIGNATURE
    );
    assert_eq!(
        seen[1].body["contents"][2]["parts"][0]["functionResponse"]["name"],
        TOOL
    );
    assert_google_request(&seen[0].body);
    assert_google_endpoint(&seen[0]);
    assert!(
        seen.iter()
            .all(|request| !request.body.to_string().contains("cache_control"))
    );
}

fn assert_google_endpoint(seen: &super::support::Seen) {
    // pin test: literal is the contract
    assert_eq!(
        seen.path,
        format!("/v1beta/models/{MODEL}:streamGenerateContent")
    );
    assert!(seen.headers.get("authorization").is_none());
    assert!(
        !seen
            .raw
            .windows(KEY.len())
            .any(|window| window == KEY.as_bytes())
    );
}

fn assert_google_request(body: &Value) {
    assert_eq!(body["tools"][0]["functionDeclarations"][0]["name"], TOOL);
}

#[tokio::test]
async fn test_native_gemini_preserves_signed_answer_parts() {
    let mut fake = Fake::serve(vec![
        gemini(&json!([{"text":ANSWER,"thoughtSignature":SIGNATURE}])),
        gemini(&json!([{"text":ANSWER}])),
    ])
    .await;
    let connector = connector(&fake, InferenceApi::Gemini);
    let chunks = streamed(&connector, GOOGLE, &[Message::User(QUESTION.to_owned())]).await;
    let replay = chunks
        .into_iter()
        .find_map(|chunk| match chunk {
            Chunk::End(end) => Some(end.replay),
            _ => None,
        })
        .unwrap();
    let messages = [
        Message::User(QUESTION.to_owned()),
        Message::Assistant {
            text: ANSWER.to_owned(),
            calls: Vec::new(),
            replay,
        },
        Message::User(QUESTION.to_owned()),
    ];
    streamed(&connector, GOOGLE, &messages).await;
    let seen = fake.seen();
    assert_eq!(seen[1].body["contents"][1]["parts"][0]["text"], ANSWER);
    assert_eq!(
        seen[1].body["contents"][1]["parts"][0]["thoughtSignature"],
        SIGNATURE
    );
}

#[tokio::test]
async fn test_native_ollama_streams_and_continues_the_exact_model_tag() {
    let mut fake = Fake::serve(vec![
        ollama(&json!({"role":"assistant","content":"","tool_calls":[{
            "function":{"name":TOOL,"arguments":{}}}]})),
        ollama(&json!({"role":"assistant","content":ANSWER})),
    ])
    .await;
    let connector = connector(&fake, InferenceApi::Ollama);
    let chunks = streamed(&connector, OLLAMA, &[Message::User(QUESTION.to_owned())]).await;
    let messages = continuation(chunks);
    streamed(&connector, OLLAMA, &messages).await;
    let seen = fake.seen();
    assert_eq!(seen.len(), 2);
    // pin test: literal is the contract
    assert!(seen.iter().all(|request| request.path == "/api/chat"));
    assert!(seen.iter().all(|request| request.body["model"] == MODEL));
    assert_eq!(seen[0].headers["authorization"], format!("Bearer {KEY}"));
    assert_eq!(
        seen[1].body["messages"][2]["tool_calls"][0]["function"]["name"],
        TOOL
    );
    assert_eq!(seen[1].body["messages"][3]["role"], "tool");
    assert!(
        seen.iter()
            .all(|request| request.body.get("cache_control").is_none())
    );
}
