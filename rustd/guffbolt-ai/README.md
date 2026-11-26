# guffbolt-ai

The provider side of the [guffbolt](https://crates.io/crates/guffbolt) agent
harness.

It holds the types one model call reads and writes, and the `Model` trait a
provider implements:

- `Request`: what one step asks the model.
- `Chunk`: one piece of the streamed answer, ending with `Chunk::End`.
- `Message`, `Call` and `Replay`: the conversation a step continues.
- `ToolSpec`, `Image` and `ImageKind`: a tool as the model is offered it, and
  an image a call hands back.
- `Usage`: the tokens one step spent.

guffbolt depends on this crate and re-exports every type under its own path.
A provider crate implements `Model` with this crate alone. A loop drives it
with guffbolt.

## Providers

Each provider is a feature. Feature `all` turns on every one.

| Feature | Wire | Key variable |
|---|---|---|
| `anthropic` | Anthropic Messages | `ANTHROPIC_API_KEY` |
| `openai` | OpenAI Responses | `OPENAI_API_KEY` |
| `gemini` | Gemini `GenerateContent` | `GEMINI_API_KEY` |
| `ollama` | Ollama chat | `OLLAMA_API_KEY`, optional |
| `openrouter` | Chat completions | `OPENROUTER_API_KEY` |
| `fireworks` | Chat completions | `FIREWORKS_API_KEY` |
| `minimax` | Anthropic Messages | `MINIMAX_API_KEY` |
| `deepseek` | Chat completions | `DEEPSEEK_API_KEY` |
| `groq` | Chat completions | `GROQ_API_KEY` |
| `mistral` | Chat completions | `MISTRAL_API_KEY` |
| `xai` | Chat completions | `XAI_API_KEY` |
| `together` | Chat completions | `TOGETHER_API_KEY` |

`Models::new().with(providers::minimax())` registers one provider.
`Models::builtin()` registers every provider the build has.
`get(provider, id)` answers the model's catalogue row.
`connect(provider, id)` reads the key from the provider's variable.
`connect_with_key` takes the key instead.
A missing key fails with the variable's name and never its value.
`Provider::with_base_url` sends a provider's requests to a proxy or a regional host.

## Quick start

With guffbolt, a connected model answers a prompt in a few lines. guffbolt carries this program as `examples/minimax_chat.rs`.

```rust,no_run
use guffbolt::{Chat, Ending};
use guffbolt_ai::{Models, providers};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), guffbolt_ai::Error> {
    // MINIMAX_API_KEY is read from the environment.
    let models = Models::new().with(providers::minimax());
    let model = models.connect("minimax", "MiniMax-M3.1-Flash")?;

    let mut chat = Chat::new(model, "Answer in one sentence.");
    match chat.prompt("Which crate parses TOML in Rust?").await {
        Ending::Answered(text) => println!("{text}"),
        Ending::Failed(why) => eprintln!("the model failed: {why}"),
        Ending::Stopped | Ending::Waiting(_) => {}
    }
    Ok(())
}
```

## Catalogue

Each provider's models are rows in `catalog/<provider>.json`, compiled in.
A row holds the model id, its context window, its output limit, and whether
it reads images.

The rows come from rig-core 0.47's bundled catalogue. They hold the
tool-calling models with both limits, and no deprecated ones. rig has no
Fireworks rows, so `Provider::with_spec` adds a Fireworks model.

Gemini's rows follow Google's model and deprecation pages instead. They add
`gemini-4-neon` and leave out the agent, Live, image and shut-down models.
Gemini 4 output stops at 65,536 tokens per streamed request.

A model no provider names is reached with `wire::Route::new`. It names the
API, base URL, dialect and limits, and `wire::Connected::new` adds the key and
transport.

## Transport

A connected model sends through the default `reqwest` client. That client
follows no redirect, so the key reaches only the provider's host.
`Connected::with_transport` replaces it with your own `transport::Send`.
`transport::Reqwest::with_client` takes your own client, which must follow no
redirect either.

The default client reads `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY` as
`reqwest` does. To name a proxy yourself, build a client with a
`reqwest::Proxy` and no redirect, then pass it to `Reqwest::with_client`.
The guffbolt README shows it under "Through a proxy".

## Features

The default build is the types alone, so a provider crate links no wire.

| Feature | What it adds |
|---|---|
| `wire` | `Route`, `Connected`, retries, timeouts and the transport seam |
| `reqwest` | the default transport, `Models`, `Provider` and `Spec` |
| one per provider, and `all` | that provider's factory and catalogue rows |
| `log-filter` | a filter that keeps rig's own log lines quiet |
| `test-util` | a fake provider's TLS listener and a client that trusts it |
