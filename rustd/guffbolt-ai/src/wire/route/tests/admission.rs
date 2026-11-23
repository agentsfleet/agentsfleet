//! Catalogue-wide admission proofs; local estimates remain distinct from native counts.

use super::{CompletionRequest, InferenceApi, Message, ModelInference, routed, settings};
use rig_core::completion::{ToolAnnotations, ToolDefinition};
use rig_core::message::ToolName;
use serde_json::json;

const OUTPUT: u32 = 37;
const REMAINING: u32 = 7;
const PHYSICAL_WINDOW: u32 = 1_000_000;
const CORPUS: [&str; 6] = [
    "hello world",
    "fn main() { println!(\"hello\"); }\n",
    "{\"path\":\"src/main.rs\",\"lines\":[1,2,3]}",
    "2026-10-25 ERROR status=413 path=/v1/messages\r\n",
    "Cafe\u{301} 🌿 你好 नमस्ते العربية",
    "<|endoftext|><|im_start|>[INST]</s><tool_call>",
];

/// A catalogue of hosted routes, as one snapshot of an embedder's held them: model
/// name, API, base URL and dialect.
const CATALOGUE: [(&str, InferenceApi, &str, Option<&str>); 20] = [
    (
        "claude-fable-5-1",
        InferenceApi::Messages,
        "https://api.anthropic.com/v1",
        None,
    ),
    (
        "claude-opus-5-5",
        InferenceApi::Messages,
        "https://api.anthropic.com/v1",
        None,
    ),
    (
        "claude-sonnet-5-5",
        InferenceApi::Messages,
        "https://api.anthropic.com/v1",
        None,
    ),
    (
        "claude-haiku-5-5",
        InferenceApi::Messages,
        "https://api.anthropic.com/v1",
        None,
    ),
    (
        "gpt-6.1-sol",
        InferenceApi::Responses,
        "https://api.openai.com/v1",
        Some("openai"),
    ),
    (
        "gpt-6-astra",
        InferenceApi::Responses,
        "https://api.openai.com/v1",
        Some("openai"),
    ),
    (
        "gpt-6-luna",
        InferenceApi::Responses,
        "https://api.openai.com/v1",
        Some("openai"),
    ),
    (
        "gpt-5.6-terra",
        InferenceApi::Responses,
        "https://api.openai.com/v1",
        Some("openai"),
    ),
    (
        "accounts/fireworks/models/minimax-m3p1-flash",
        InferenceApi::Chat,
        "https://api.fireworks.ai/inference/v1",
        None,
    ),
    (
        "accounts/fireworks/models/glm-5p3-flash",
        InferenceApi::Chat,
        "https://api.fireworks.ai/inference/v1",
        None,
    ),
    (
        "accounts/fireworks/models/kimi-k3",
        InferenceApi::Chat,
        "https://api.fireworks.ai/inference/v1",
        None,
    ),
    (
        "accounts/fireworks/models/deepseek-v4p1-flash",
        InferenceApi::Chat,
        "https://api.fireworks.ai/inference/v1",
        None,
    ),
    (
        "meta/muse-wonder",
        InferenceApi::Chat,
        "https://openrouter.ai/api/v1",
        Some("openrouter"),
    ),
    (
        "google/gemini-4-argon",
        InferenceApi::Chat,
        "https://openrouter.ai/api/v1",
        Some("openrouter"),
    ),
    (
        "qwen/qwen3.8-max-0902",
        InferenceApi::Chat,
        "https://openrouter.ai/api/v1",
        Some("openrouter"),
    ),
    (
        "minimax/minimax-m3.1-flash:free",
        InferenceApi::Chat,
        "https://openrouter.ai/api/v1",
        Some("openrouter"),
    ),
    (
        "openai/gpt-oss-120b",
        InferenceApi::Chat,
        "https://api.groq.com/openai/v1",
        Some("groq"),
    ),
    (
        "mistral-large-4",
        InferenceApi::Chat,
        "https://api.mistral.ai",
        Some("mistral"),
    ),
    (
        "mistral-small-2603",
        InferenceApi::Chat,
        "https://api.mistral.ai",
        Some("mistral"),
    ),
    (
        "gemini-4-argon",
        InferenceApi::Gemini,
        "https://generativelanguage.googleapis.com",
        None,
    ),
];

fn catalogue() -> Vec<(String, ModelInference)> {
    let mut routes: Vec<_> = CATALOGUE
        .iter()
        .map(|&(model, api, base_url, dialect)| {
            (
                model.to_owned(),
                ModelInference {
                    api,
                    base_url: base_url.into(),
                    dialect: dialect.map(str::to_owned),
                    max_output_tokens: OUTPUT,
                },
            )
        })
        .collect();
    routes.push((
        "configured-local-model:latest".into(),
        settings(InferenceApi::Ollama),
    ));
    routes.push((
        "unknown-provider/model-next".into(),
        settings(InferenceApi::Chat),
    ));
    routes
}

fn request(text: &str) -> CompletionRequest {
    CompletionRequest::new(Message::user(text)).max_tokens(u64::from(OUTPUT))
}

// Value: protects=every supported route reserves its complete current request;
// seam=the production streaming encoder, estimator and output admission together.
#[test]
fn supported_models_reserve_complete_requests_before_output() {
    for (model, settings) in catalogue() {
        for text in CORPUS {
            let body = request(text);
            let route = routed(&settings, PHYSICAL_WINDOW).unwrap();
            let input = u32::try_from(route.measure_input(&model, &body).unwrap()).unwrap();
            assert!(input > 0, "{model}");
            let route = routed(&settings, input + REMAINING).unwrap();
            let bounded = route.bound_output(&model, body).unwrap();
            assert_eq!(bounded.max_tokens, Some(u64::from(REMAINING)), "{model}");
        }
    }
}

#[test]
fn every_supported_route_refuses_input_that_leaves_no_output() {
    for (model, settings) in catalogue() {
        let body = request(&CORPUS.join("\n"));
        let route = routed(&settings, PHYSICAL_WINDOW).unwrap();
        let input = u32::try_from(route.measure_input(&model, &body).unwrap()).unwrap();
        for window in [input, input - 1] {
            let route = routed(&settings, window).unwrap();
            let error = route.bound_output(&model, body.clone()).unwrap_err();
            assert!(error.is_invalid_request(), "{model}");
            assert!(error.to_string().contains("context"), "{model}");
        }
    }
}

#[test]
fn tool_schemas_and_pinned_instructions_cannot_be_omitted_from_admission() {
    for (model, settings) in catalogue() {
        let route = routed(&settings, PHYSICAL_WINDOW).unwrap();
        let plain = request(CORPUS[0]);
        let baseline = route.measure_input(&model, &plain).unwrap();
        let mut tools = plain.clone();
        tools.tools.push(
            ToolDefinition {
                annotations: ToolAnnotations::default(),
                name: ToolName::new("inspect_file").unwrap(),
                description: CORPUS.join("\n").repeat(16),
                parameters: json!({"type":"object","properties":{
                "path":{"type":"string","description":CORPUS.join("\n").repeat(16)}
            },"required":["path"]}),
            }
            .into(),
        );
        let pinned = plain.preamble(CORPUS.join("\n").repeat(32));
        assert!(
            route.measure_input(&model, &tools).unwrap() > baseline,
            "{model}"
        );
        assert!(
            route.measure_input(&model, &pinned).unwrap() > baseline,
            "{model}"
        );
    }
}

#[test]
fn switching_to_an_unknown_tokenizer_cannot_reuse_an_openai_count() {
    let settings = settings(InferenceApi::Chat);
    let route = routed(&settings, PHYSICAL_WINDOW).unwrap();
    let body = request(&"Repeated source code and logs.\n".repeat(256));
    let known = route.measure_input("gpt-5.6-terra", &body).unwrap();
    for model in [
        "accounts/fireworks/models/minimax-m3p1-flash",
        "minimax/minimax-m3.1-flash:free",
        "unknown-provider/model-next",
    ] {
        let switched = route.measure_input(model, &body).unwrap();
        // The proxy adds a quarter to the `o200k_base` count this body has
        // under `gpt-5.6-terra`, so the switched estimate is strictly above it.
        assert!(switched > known, "{model}: {switched} <= {known}");
        assert_eq!(route.measure_input("gpt-5.6-terra", &body).unwrap(), known);
    }
}

#[test]
fn literal_special_tokens_never_become_installed_control_tokens() {
    let text = CORPUS[5];
    let ordinary = crate::wire::input_tokens("gpt-4o", text);
    assert_eq!(ordinary, 19);
    // Equal only if the proxy also reads special-token spellings as text: an
    // installed control token would collapse each into one and count lower.
    assert_eq!(
        crate::wire::input_tokens("unknown-provider/model-next", text),
        24
    );
}
