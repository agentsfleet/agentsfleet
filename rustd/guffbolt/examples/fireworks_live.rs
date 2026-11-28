//! A live run on Fireworks' GLM 5.3 Flash, by hand only: guffbolt-ai alone,
//! then guffbolt over it. No test target, workflow or make recipe runs it
//! (`M253_004` Dimension 13.2), so no key ever reaches a CI secret.
//!
//! 1. guffbolt-ai alone: one step streamed through [`Model::stream`], each
//!    chunk printed as it arrives.
//! 2. The step's [`Chunk::Usage`], read back as tokens.
//! 3. guffbolt's [`Chat`] over the same model: one prompt answered.
//! 4. A tool call's round trip: the model calls `add`, and the answer carries
//!    its result.
//!
//! `FIREWORKS_API_KEY` holds the key; the program never prints it, and a
//! missing one fails naming the variable.
//!
//! ```text
//! FIREWORKS_API_KEY=… cargo run -q --locked -p guffbolt --example fireworks_live
//! ```

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use guffbolt::{Chat, Describe, Ending, Tool, ToolFuture, ToolOutput, Tools};
use guffbolt_ai::{Chunk, Message, Model, Models, Request, Spec, TextKind, ToolSpec, providers};
use serde_json::{Value, json};

const MODEL: &str = "accounts/fireworks/models/glm-5p3-flash";

// Fireworks' serverless price table for GLM 5.3 Flash
// (fireworks.ai/models/fireworks/glm-5p3-flash, read Nov 22, 2026): $0.15 per
// million input tokens and $0.50 per million output tokens, in nano-dollars
// per token. Every prompt token is priced as fresh input, the table's dearer
// rate, so the tally never reads low.
const INPUT_NANO_DOLLARS: u64 = 150;
const OUTPUT_NANO_DOLLARS: u64 = 500;

// $2.00: the most Indy allowed one manual run to spend (Nov 22, 2026). It is
// this program's safety stop, not a cap the product sets on a provider.
const BUDGET_NANO_DOLLARS: u64 = 2_000_000_000;

/// Why a metered step failed: the budget was reached before it began, or
/// the provider failed it.
#[derive(Debug)]
enum Halt {
    Budget,
    Provider(guffbolt_ai::Error),
}

impl fmt::Display for Halt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Budget => f.write_str("the $2.00 budget is spent; no call is sent"),
            Self::Provider(why) => why.fmt(f),
        }
    }
}

impl std::error::Error for Halt {}

/// A connected model whose every step adds its reported usage to one shared
/// tally, and which sends no step once the tally reaches the budget.
#[derive(Debug)]
struct Metered {
    model: guffbolt_ai::wire::Connected,
    spent: Arc<AtomicU64>,
}

fn dollars(nano: u64) -> String {
    format!("${}.{:09}", nano / 1_000_000_000, nano % 1_000_000_000)
}

impl Model for Metered {
    type Error = Halt;

    fn measure(&self, request: Request<'_>) -> Result<u64, Halt> {
        self.model.measure(request).map_err(Halt::Provider)
    }

    #[expect(
        clippy::print_stdout,
        reason = "the running total is the example's output"
    )]
    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk, Halt>> {
        if self.spent.load(Ordering::SeqCst) >= BUDGET_NANO_DOLLARS {
            return stream::once(async { Err(Halt::Budget) }).boxed();
        }
        self.model
            .stream(request)
            .map(|chunk| {
                if let Ok(Chunk::Usage(usage)) = &chunk {
                    let cost =
                        usage.prompt() * INPUT_NANO_DOLLARS + usage.output * OUTPUT_NANO_DOLLARS;
                    let total = self.spent.fetch_add(cost, Ordering::SeqCst) + cost;
                    println!(
                        "  [spend] in {} out {}: {} this call, {} so far",
                        usage.prompt(),
                        usage.output,
                        dollars(cost),
                        dollars(total)
                    );
                }
                chunk.map_err(Halt::Provider)
            })
            .boxed()
    }

    fn accepts_images(&self) -> bool {
        self.model.accepts_images()
    }
}

/// Adds two integers, and counts the calls it answers.
#[derive(Debug)]
struct Add {
    parameters: Value,
    calls: Arc<AtomicU64>,
}

impl Describe for Add {
    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: "add",
            description: "Adds two integers a and b.",
            parameters: &self.parameters,
        }
    }
}

impl Tool<()> for Add {
    #[expect(
        clippy::print_stdout,
        reason = "the call it ran is the example's output"
    )]
    fn call<'a>(&'a self, arguments: &'a Value, _cx: ()) -> ToolFuture<'a>
    where
        (): 'a,
    {
        let (a, b) = (arguments["a"].as_i64(), arguments["b"].as_i64());
        self.calls.fetch_add(1, Ordering::SeqCst);
        println!("  [tool] add({arguments})");
        Box::pin(async move {
            match (a, b) {
                (Some(a), Some(b)) => ToolOutput::succeeded((a + b).to_string()),
                _ => ToolOutput::succeeded("a and b must both be integers"),
            }
        })
    }
}

#[expect(clippy::print_stderr, reason = "a failure is told on stderr")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    // An unset or empty FIREWORKS_API_KEY fails the first connect, and its
    // error names the variable, never a value.
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("fireworks_live failed: {why}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[expect(clippy::print_stdout, reason = "the example's output is its answers")]
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Fireworks' model record names a 1,048,576-token context and no output
    // limit, nor does its model page, so the context window is the ceiling.
    let spec = Spec {
        id: MODEL.to_owned(),
        context_tokens: 1_048_576,
        max_output_tokens: 1_048_576,
        images: true,
    };
    let models = Models::new().with(providers::fireworks().with_spec(spec));
    let spent = Arc::new(AtomicU64::new(0));
    let metered = || -> Result<Metered, guffbolt_ai::Error> {
        Ok(Metered {
            model: models.connect("fireworks", MODEL)?,
            spent: Arc::clone(&spent),
        })
    };

    println!("1. guffbolt-ai alone: one streamed step");
    let model = metered()?;
    let messages = [Message::User(
        "Name three primary colours, comma separated.".to_owned(),
    )];
    let request = Request {
        model: MODEL,
        instructions: "Answer in one short line.",
        messages: &messages,
        tools: &[],
        cache_key: "fireworks-live",
        max_output_tokens: Some(256),
        single_attempt: false,
    };
    let (mut usage, mut reasoning) = (None, 0);
    let mut chunks = model.stream(request);
    while let Some(chunk) = chunks.next().await {
        match chunk? {
            Chunk::Text {
                kind: TextKind::Answer,
                text,
            } => println!("  [chunk] {text:?}"),
            // Reasoning is the model's own; its size is enough to show it streamed.
            Chunk::Text {
                kind: TextKind::Reasoning,
                text,
            } => reasoning += text.len(),
            Chunk::Call(call) => println!("  [call] {}", call.name),
            Chunk::Usage(read) => usage = Some(read),
            Chunk::End(end) => println!("  [end] {reasoning} reasoning bytes, cut: {}", end.cut),
        }
    }
    drop(chunks);

    println!("2. the step's usage");
    let usage = usage.ok_or("the step reported no usage")?;
    println!(
        "  input {} output {} total {}",
        usage.prompt(),
        usage.output,
        usage.total()
    );

    println!("3. guffbolt Chat over the same model: one prompt");
    let mut chat = Chat::new(metered()?, "Answer in one sentence.");
    match chat.prompt("Which planet is closest to the Sun?").await {
        Ending::Answered(text) => println!("  [answer] {text}"),
        other => return Err(format!("the chat did not answer: {other:?}").into()),
    }

    println!("4. a tool call's round trip");
    let calls = Arc::new(AtomicU64::new(0));
    let add = Add {
        parameters: json!({
            "type": "object",
            "properties": { "a": { "type": "integer" }, "b": { "type": "integer" } },
            "required": ["a", "b"]
        }),
        calls: Arc::clone(&calls),
    };
    let tools: Tools<dyn Tool<()>> = Tools::new(vec![Box::new(add)]);
    let mut chat = Chat::new(
        metered()?,
        "Use the add tool for any sum, then state its result.",
    )
    .with_tools(tools);
    let answer = match chat.prompt("What is 4817 plus 2396?").await {
        Ending::Answered(text) => text,
        other => return Err(format!("the tool chat did not answer: {other:?}").into()),
    };
    println!("  [answer] {answer}");
    let called = calls.load(Ordering::SeqCst);
    let carried = answer.replace(',', "").contains("7213");
    println!("  add called {called} time(s); the answer carries 7213: {carried}");
    if called == 0 || !carried {
        return Err("the tool call did not round-trip".into());
    }

    println!(
        "spent {} of {}",
        dollars(spent.load(Ordering::SeqCst)),
        dollars(BUDGET_NANO_DOLLARS)
    );
    Ok(())
}
