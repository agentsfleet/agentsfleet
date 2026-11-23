//! `examples/minimax_chat`, the program both READMEs show, run as a process
//! against a fake `MiniMax`: its key read from the environment, its one
//! prompt sent on the Messages wire, its answer printed.
//!
//! The fake serves plain HTTP on loopback. The example's default transport
//! trusts only the system's roots, so a fake's own certificate authority
//! cannot reach it, and the base URL is the one thing the program takes.

use std::path::PathBuf;
use std::process::Command;

use axum::Router;
use axum::http::{HeaderMap, StatusCode, header::CONTENT_TYPE};
use axum::response::{IntoResponse as _, Response};
use axum::routing::post;
use guffbolt_ai::testing::messages;
use tokio::net::TcpListener;

/// The variable the example reads its key from, and the key the fake admits.
const KEY_VARIABLE: &str = "MINIMAX_API_KEY";
const KEY: &str = "fixture-minimax-key";
/// The id the example connects.
const MODEL: &str = "MiniMax-M3.1-Flash";
const ANSWER: &str = "The toml crate parses TOML in Rust.";
/// Where an instrumented child writes its coverage profile.
const PROFILE_KNOB: &str = "LLVM_PROFILE_FILE";

/// The example's binary. `cargo test` builds every example into the profile
/// directory's `examples/`, and this suite runs from below that directory:
/// from `deps/`, or from `build/` in Cargo's newer layout.
fn example() -> PathBuf {
    let name = format!("minimax_chat{}", std::env::consts::EXE_SUFFIX);
    let suite = std::env::current_exe().expect("the suite knows its own path");
    suite
        .ancestors()
        .map(|directory| directory.join("examples").join(&name))
        .find(|binary| binary.is_file())
        .expect("`cargo test` built the example beside this suite")
}

/// A fake `MiniMax` on loopback that answers [`ANSWER`] to a request
/// carrying [`KEY`] and refuses any other; its base URL.
async fn fake() -> String {
    let turn = messages::answer(MODEL, ANSWER).concat();
    let app = Router::new().route(
        "/v1/messages",
        post(move |headers: HeaderMap| {
            let turn = turn.clone();
            async move {
                let admitted = headers
                    .get("x-api-key")
                    .is_some_and(|key| key.as_bytes() == KEY.as_bytes());
                let response: Response = if admitted {
                    ([(CONTENT_TYPE, "text/event-stream")], turn).into_response()
                } else {
                    StatusCode::UNAUTHORIZED.into_response()
                };
                response
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port is free");
    let address = listener.local_addr().expect("the listener has an address");
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{address}")
}

// Value: the README's program, built as it is published, reaches a provider
// through `Models`, the `minimax` factory and `Chat` on the real wire, and
// prints what the provider answered.
#[tokio::test]
#[expect(
    clippy::disallowed_methods,
    reason = "the coverage profile path is passed through as raw OS bytes, as the runner's binary suite passes it"
)]
async fn the_minimax_example_answers_against_a_fake_provider() {
    let base = fake().await;
    let mut command = Command::new(example());
    command.arg(&base).env_clear().env(KEY_VARIABLE, KEY);
    if let Some(profile) = std::env::var_os(PROFILE_KNOB) {
        command.env(PROFILE_KNOB, profile);
    }

    let output = tokio::task::spawn_blocking(move || command.output())
        .await
        .expect("the example's wait joins")
        .expect("the example starts; `cargo test` builds it");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("{ANSWER}\n"),
        "{output:?}"
    );
}
