//! The plain `MiniMax` chat both READMEs show, as a program: one prompt, one
//! answer on stdout.
//!
//! `MINIMAX_API_KEY` holds the key. An optional first argument replaces
//! `MiniMax`'s base URL, so a test points the program at a fake provider;
//! without it the program talks to `MiniMax`.
//!
//! ```text
//! MINIMAX_API_KEY=… cargo run -p guffbolt --example minimax_chat [base-url]
//! ```

use guffbolt::{Chat, Ending};
use guffbolt_ai::{Models, providers};

#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the example's output is its answer, and a failure is told on stderr"
)]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), guffbolt_ai::Error> {
    // MINIMAX_API_KEY is read from the environment.
    let mut minimax = providers::minimax();
    if let Some(base_url) = std::env::args().nth(1) {
        minimax = minimax.with_base_url(base_url);
    }
    let models = Models::new().with(minimax);
    let model = models.connect("minimax", "MiniMax-M3.1-Flash")?;

    let mut chat = Chat::new(model, "Answer in one sentence.");
    match chat.prompt("Which crate parses TOML in Rust?").await {
        Ending::Answered(text) => println!("{text}"),
        Ending::Failed(why) => eprintln!("the model failed: {why}"),
        Ending::Stopped | Ending::Waiting(_) => {}
    }
    Ok(())
}
