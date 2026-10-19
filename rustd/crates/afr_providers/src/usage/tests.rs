use rig_core::completion::Usage as Reported;
use serde_json::{Value, json};

use super::spent;
use crate::provider::Usage;
use afd_wire::inference::InferenceApi;

/// The prompt every turn here sends: 500 fresh, 400 read, 100 written.
const PROMPT: u64 = 1_000;
const FRESH: u64 = 500;
const READ: u64 = 400;
const WRITTEN: u64 = 100;
/// The completion, of which [`REASONING`] was reasoning.
const OUTPUT: u64 = 200;
const REASONING: u64 = 50;
/// How a Messages reply splits [`WRITTEN`] by lifetime.
const FIVE_MINUTES: u64 = 60;
const ONE_HOUR: u64 = 40;

/// The usage rig reports for a prompt of [`PROMPT`] that read [`READ`] and
/// wrote `written`, every wire: input counts the reads and the writes.
fn reported(written: Option<u64>) -> Reported {
    Reported::new()
        .input_tokens(PROMPT)
        .cached_input_tokens(READ)
        .cache_creation_input_tokens(written)
        .output_tokens(OUTPUT)
        .reasoning_tokens(REASONING)
        .total_tokens(PROMPT + OUTPUT)
}

/// A Messages reply as Anthropic sends it, reassembled: its `usage` counts
/// fresh input beside the reads and writes, and splits the writes by
/// lifetime.
fn messages_reply(cache_creation: &Value) -> Value {
    json!({
        "id": "msg_01XFDUDYJgAACzvnptvVoYEL",
        "type": "message",
        "role": "assistant",
        "model": "claude-sonnet-4-6",
        "content": [{"type": "text", "text": "Done."}],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {
            "input_tokens": FRESH,
            "cache_read_input_tokens": READ,
            "cache_creation_input_tokens": WRITTEN,
            "cache_creation": cache_creation,
            "output_tokens": OUTPUT,
            "service_tier": "standard"
        }
    })
}

/// A Responses reply as `OpenAI` sends it: writes counted inside
/// `input_tokens`, with no lifetime.
fn responses_reply() -> Value {
    json!({
        "id": "resp_67ccd2bed1ec8190b14f964abc054267",
        "object": "response",
        "status": "completed",
        "model": "gpt-5.6",
        "output": [],
        "usage": {
            "input_tokens": PROMPT,
            "input_tokens_details": {"cached_tokens": READ, "cache_write_tokens": WRITTEN},
            "output_tokens": OUTPUT,
            "output_tokens_details": {"reasoning_tokens": REASONING},
            "total_tokens": PROMPT + OUTPUT
        }
    })
}

/// 1,000 prompt tokens, of which 400 were read from the cache and 100 written
/// to it, and a completion holding 50 reasoning tokens: 500 uncached, 400 read,
/// 100 written, and the completion once.
#[test]
fn test_usage_counts_each_token_once() {
    let usage = spent(InferenceApi::Chat, &reported(Some(WRITTEN)), &Value::Null);

    assert_eq!(
        usage,
        Usage {
            input: FRESH,
            cached_input: READ,
            cache_write: WRITTEN,
            cache_write_5m: 0,
            cache_write_1h: 0,
            output: OUTPUT,
        }
    );
    assert_eq!(usage.prompt(), PROMPT, "the prompt, each token once");
    assert_eq!(
        usage.total(),
        PROMPT + OUTPUT,
        "reasoning is not added again"
    );
}

/// A Messages reply's writes bill by the lifetime it names, and none is left
/// as a write of no lifetime.
#[test]
fn messages_writes_split_by_lifetime() {
    let split = json!({"ephemeral_5m_input_tokens": FIVE_MINUTES,
        "ephemeral_1h_input_tokens": ONE_HOUR});
    let raw = messages_reply(&split);

    let usage = spent(InferenceApi::Messages, &reported(Some(WRITTEN)), &raw);

    assert_eq!(
        (usage.input, usage.cached_input, usage.output),
        (FRESH, READ, OUTPUT)
    );
    assert_eq!(
        (
            usage.cache_write,
            usage.cache_write_5m,
            usage.cache_write_1h
        ),
        (0, FIVE_MINUTES, ONE_HOUR)
    );
}

/// A Messages gateway that names no lifetime, or names one this cannot read,
/// leaves its writes as writes of no lifetime: never guessed into five
/// minutes.
#[test]
fn messages_writes_with_no_readable_split_have_no_lifetime() {
    let replies = [
        messages_reply(&Value::Null),
        messages_reply(&json!("ephemeral")),
        json!({"usage": {"input_tokens": FRESH}}),
    ];
    for raw in replies {
        let usage = spent(InferenceApi::Messages, &reported(Some(WRITTEN)), &raw);

        assert_eq!(
            (
                usage.cache_write,
                usage.cache_write_5m,
                usage.cache_write_1h
            ),
            (WRITTEN, 0, 0),
            "{raw}"
        );
        assert_eq!(usage.input, FRESH, "{raw}");
    }
}

/// A split whose lifetimes sum past the write total rig read keeps the
/// split's counts, and takes from input only what rig put in it.
#[test]
fn messages_split_past_the_total_is_not_taken_from_input() {
    let split = json!({"ephemeral_5m_input_tokens": FIVE_MINUTES,
        "ephemeral_1h_input_tokens": ONE_HOUR});
    let raw = messages_reply(&split);
    let unreported = Reported::new()
        .input_tokens(FRESH + READ)
        .cached_input_tokens(READ)
        .output_tokens(OUTPUT);

    let usage = spent(InferenceApi::Messages, &unreported, &raw);

    assert_eq!(usage.input, FRESH);
    assert_eq!(
        (
            usage.cache_write,
            usage.cache_write_5m,
            usage.cache_write_1h
        ),
        (0, FIVE_MINUTES, ONE_HOUR)
    );
}

/// Responses and Chat report a write with no lifetime; a reply on any wire
/// but Messages is never read for one, whatever its body holds.
#[test]
fn other_wires_report_writes_with_no_lifetime() {
    let split = json!({"ephemeral_5m_input_tokens": FIVE_MINUTES,
        "ephemeral_1h_input_tokens": ONE_HOUR});
    let wires = [
        (InferenceApi::Responses, responses_reply()),
        (InferenceApi::Chat, messages_reply(&split)),
        (InferenceApi::Gemini, messages_reply(&split)),
        (InferenceApi::Ollama, messages_reply(&split)),
    ];
    for (api, raw) in wires {
        let usage = spent(api, &reported(Some(WRITTEN)), &raw);

        assert_eq!(
            (
                usage.cache_write,
                usage.cache_write_5m,
                usage.cache_write_1h
            ),
            (WRITTEN, 0, 0),
            "{api:?}"
        );
        assert_eq!(usage.input, FRESH, "{api:?}");
    }
}

/// A wire that reports no write, Gemini's implicit cache among them, keeps
/// every unread prompt token as input.
#[test]
fn no_reported_write_leaves_input_whole() {
    let usage = spent(InferenceApi::Gemini, &reported(None), &Value::Null);

    assert_eq!(usage.input, PROMPT - READ);
    assert_eq!(usage.cache_written(), 0);
}

/// Usage that reports nothing spends nothing.
#[test]
fn unreported_usage_is_zero() {
    let usage = spent(InferenceApi::Messages, &Reported::new(), &Value::Null);

    assert_eq!(usage, Usage::default());
}
