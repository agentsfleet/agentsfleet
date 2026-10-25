//! Catalogue-wide admission proofs; local estimates remain distinct from native counts.

use super::{CompletionRequest, Configured, InferenceApi, Message, ModelInference, settings};
use rig_core::completion::{ToolAnnotations, ToolDefinition};
use rig_core::message::ToolName;
use serde_json::{Value, json};

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

fn catalogue() -> Vec<(String, ModelInference)> {
    let catalogue: Value = serde_json::from_str(include_str!("fixtures/catalogue.json")).unwrap();
    let mut routes: Vec<_> = catalogue
        .get("models")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let mut settings: ModelInference =
                serde_json::from_value(entry.get("inference").unwrap().clone()).unwrap();
            settings.max_output_tokens = OUTPUT;
            (
                entry
                    .get("model_name")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned(),
                settings,
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
            let route = Configured::new(&settings, PHYSICAL_WINDOW).unwrap();
            let input = u32::try_from(route.measure_input(&model, &body).unwrap()).unwrap();
            assert!(input > 0, "{model}");
            let route = Configured::new(&settings, input + REMAINING).unwrap();
            let bounded = route.bound_output(&model, body).unwrap();
            assert_eq!(bounded.max_tokens, Some(u64::from(REMAINING)), "{model}");
        }
    }
}

#[test]
fn every_supported_route_refuses_input_that_leaves_no_output() {
    for (model, settings) in catalogue() {
        let body = request(&CORPUS.join("\n"));
        let route = Configured::new(&settings, PHYSICAL_WINDOW).unwrap();
        let input = u32::try_from(route.measure_input(&model, &body).unwrap()).unwrap();
        for window in [input, input - 1] {
            let route = Configured::new(&settings, window).unwrap();
            let error = route.bound_output(&model, body.clone()).unwrap_err();
            assert_eq!(
                error.code(),
                afd_core::error_code::INVALID_REQUEST,
                "{model}"
            );
            assert!(error.to_string().contains("context"), "{model}");
        }
    }
}

#[test]
fn tool_schemas_and_pinned_instructions_cannot_be_omitted_from_admission() {
    for (model, settings) in catalogue() {
        let route = Configured::new(&settings, PHYSICAL_WINDOW).unwrap();
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
    let route = Configured::new(&settings, PHYSICAL_WINDOW).unwrap();
    let body = request(&"Repeated source code and logs.\n".repeat(256));
    let known = route.measure_input("gpt-5.6-terra", &body).unwrap();
    for model in [
        "accounts/fireworks/models/minimax-m3p1-flash",
        "minimax/minimax-m3.1-flash:free",
        "unknown-provider/model-next",
    ] {
        let switched = route.measure_input(model, &body).unwrap();
        assert!(switched > known * 2, "{model}");
        assert_eq!(route.measure_input("gpt-5.6-terra", &body).unwrap(), known);
    }
}

#[test]
fn literal_special_tokens_never_become_installed_control_tokens() {
    let text = CORPUS[5];
    assert!(crate::input_tokens("gpt-4o", text) > 6);
    assert_eq!(
        crate::input_tokens("unknown-provider/model-next", text),
        text.len()
    );
}
