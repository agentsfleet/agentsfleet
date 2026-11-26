//! One factory per provider, each behind the feature of its name; feature
//! `all` turns every one on.
//!
//! | Factory | Wire, rig dialect | Key variable |
//! |---|---|---|
//! | [`anthropic`] | Messages, `anthropic` | `ANTHROPIC_API_KEY` |
//! | [`openai`] | Responses, `openai` | `OPENAI_API_KEY` |
//! | [`gemini`] | Gemini native | `GEMINI_API_KEY` |
//! | [`ollama`] | Ollama native | `OLLAMA_API_KEY`, optional |
//! | [`openrouter`] | Chat, `openrouter` | `OPENROUTER_API_KEY` |
//! | [`fireworks`] | Chat, gateway | `FIREWORKS_API_KEY` |
//! | [`minimax`] | Messages, `minimax` | `MINIMAX_API_KEY` |
//! | [`deepseek`], [`groq`], [`mistral`], [`xai`], [`together`] | Chat, rig's of that name | rig's |
//!
//! A provider's base URL and key variable are rig's, where rig names them.
//! Its rows are `catalog/<provider>.json`, compiled in: the tool-calling
//! models rig-core 0.47's bundled catalogue lists for that provider with a
//! context window and an output limit, deprecated ones left out. rig's
//! catalogue has no Fireworks rows, so [`fireworks`] serves only the rows
//! [`Provider::with_spec`] adds. Gemini's rows follow Google's model and
//! deprecation pages instead: they add `gemini-4-neon`, leave out the agent,
//! Live, image and shut-down models, and stop Gemini 4 output at 65,536 tokens,
//! the per-request limit of the streamed calls this crate sends.

use crate::models::Provider;

/// Every provider this build compiled in.
pub(crate) fn compiled() -> Vec<Provider> {
    Vec::from([
        #[cfg(feature = "anthropic")]
        anthropic(),
        #[cfg(feature = "openai")]
        openai(),
        #[cfg(feature = "gemini")]
        gemini(),
        #[cfg(feature = "ollama")]
        ollama(),
        #[cfg(feature = "openrouter")]
        openrouter(),
        #[cfg(feature = "fireworks")]
        fireworks(),
        #[cfg(feature = "minimax")]
        minimax(),
        #[cfg(feature = "deepseek")]
        deepseek(),
        #[cfg(feature = "groq")]
        groq(),
        #[cfg(feature = "mistral")]
        mistral(),
        #[cfg(feature = "xai")]
        xai(),
        #[cfg(feature = "together")]
        together(),
    ])
}

/// Anthropic, on its Messages API.
#[cfg(feature = "anthropic")]
#[must_use]
pub fn anthropic() -> Provider {
    let dialect = rig_core::providers::anthropic::wire::ANTHROPIC;
    Provider::new(
        "anthropic",
        crate::wire::Api::Messages,
        Some(dialect.name),
        dialect.base_url,
        (dialect.api_key_env, true),
        include_str!("../../catalog/anthropic.json"),
    )
}

/// `OpenAI`, on its Responses API.
#[cfg(feature = "openai")]
#[must_use]
pub fn openai() -> Provider {
    let dialect = &rig_core::providers::openai::wire::OPENAI;
    Provider::new(
        "openai",
        crate::wire::Api::Responses,
        Some(dialect.name),
        dialect.base_url,
        (dialect.api_key_env, true),
        include_str!("../../catalog/openai.json"),
    )
}

/// Google Gemini, on its native `GenerateContent` API.
#[cfg(feature = "gemini")]
#[must_use]
pub fn gemini() -> Provider {
    use rig_core::providers::gemini::{API_KEY_ENV, BASE_URL};
    Provider::new(
        "gemini",
        crate::wire::Api::Gemini,
        None,
        BASE_URL,
        (API_KEY_ENV, true),
        include_str!("../../catalog/gemini.json"),
    )
}

/// A local Ollama server, on its native chat API. Its key is sent only when
/// `OLLAMA_API_KEY` holds one.
#[cfg(feature = "ollama")]
#[must_use]
pub fn ollama() -> Provider {
    Provider::new(
        "ollama",
        crate::wire::Api::Ollama,
        None,
        "http://localhost:11434",
        ("OLLAMA_API_KEY", false),
        include_str!("../../catalog/ollama.json"),
    )
}

/// `OpenRouter`, on chat completions.
#[cfg(feature = "openrouter")]
#[must_use]
pub fn openrouter() -> Provider {
    Provider::chat(
        "openrouter",
        &rig_core::providers::openai::wire::OPENROUTER,
        include_str!("../../catalog/openrouter.json"),
    )
}

/// Fireworks, on chat completions in rig's gateway dialect. Its catalogue is
/// empty: add a model with [`Provider::with_spec`].
#[cfg(feature = "fireworks")]
#[must_use]
pub fn fireworks() -> Provider {
    Provider::new(
        "fireworks",
        crate::wire::Api::Chat,
        None,
        "https://api.fireworks.ai/inference/v1",
        ("FIREWORKS_API_KEY", true),
        include_str!("../../catalog/fireworks.json"),
    )
}

/// `MiniMax`, on its Anthropic Messages endpoint.
///
/// ```no_run
/// use guffbolt_ai::{Models, providers};
///
/// # fn main() -> guffbolt_ai::Result<()> {
/// // MINIMAX_API_KEY is read from the environment.
/// let models = Models::new().with(providers::minimax());
/// let model = models.connect("minimax", "MiniMax-M3.1-Flash")?;
/// # drop(model);
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "minimax")]
#[must_use]
pub fn minimax() -> Provider {
    let dialect = rig_core::providers::anthropic::wire::MINIMAX;
    Provider::new(
        "minimax",
        crate::wire::Api::Messages,
        Some(dialect.name),
        dialect.base_url,
        (dialect.api_key_env, true),
        include_str!("../../catalog/minimax.json"),
    )
}

/// `DeepSeek`, on chat completions.
#[cfg(feature = "deepseek")]
#[must_use]
pub fn deepseek() -> Provider {
    Provider::chat(
        "deepseek",
        &rig_core::providers::openai::wire::DEEPSEEK,
        include_str!("../../catalog/deepseek.json"),
    )
}

/// Groq, on chat completions.
#[cfg(feature = "groq")]
#[must_use]
pub fn groq() -> Provider {
    Provider::chat(
        "groq",
        &rig_core::providers::openai::wire::GROQ,
        include_str!("../../catalog/groq.json"),
    )
}

/// Mistral, on chat completions.
#[cfg(feature = "mistral")]
#[must_use]
pub fn mistral() -> Provider {
    Provider::chat(
        "mistral",
        &rig_core::providers::openai::wire::MISTRAL,
        include_str!("../../catalog/mistral.json"),
    )
}

/// xAI, on chat completions.
#[cfg(feature = "xai")]
#[must_use]
pub fn xai() -> Provider {
    Provider::chat(
        "xai",
        &rig_core::providers::xai::DIALECT,
        include_str!("../../catalog/xai.json"),
    )
}

/// Together, on chat completions.
#[cfg(feature = "together")]
#[must_use]
pub fn together() -> Provider {
    Provider::chat(
        "together",
        &rig_core::providers::openai::wire::TOGETHER,
        include_str!("../../catalog/together.json"),
    )
}
