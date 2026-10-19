//! Which saved APIs get a tool's images to the model.

use afd_wire::inference::InferenceApi;

/// Chat completions take text alone; every other API delivers a tool's images,
/// inside the result or in the user message rig's history adapter adds.
#[test]
fn only_chat_completions_drop_a_tools_images() {
    for (api, carries) in [
        (InferenceApi::Chat, false),
        (InferenceApi::Responses, true),
        (InferenceApi::Messages, true),
        (InferenceApi::Gemini, true),
        (InferenceApi::Ollama, true),
    ] {
        assert_eq!(api.carries_images(), carries, "{api:?}");
    }
}
