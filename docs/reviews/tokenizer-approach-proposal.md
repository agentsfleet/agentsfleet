---
type: explanation
audience: contributor
verified: 2026-10-26
product_version: 0.65.0
executable: false
---

# Token counting approach proposal

Status: D1 and an interim D2 decided; D3 and D4 open.

Indy's decision on 2026-11-04 picked "o200k × 1.25" with the note “i just need an estimation for now”:

- **D1:** keep `tiktoken-rs`.
- **D2, interim:** every model without a local tokenizer is estimated as its `o200k_base` count plus a quarter (`PROXY_HEADROOM_PERCENT` in `rustd/crates/afd_inference/src/budget.rs`). This is Option A with one provisional multiplier in place of measured per-model values.
- **Per-model values later** come from the existing `model_input_calibration` trace, which records `measured_input_tokens` beside `actual_input_tokens` for every call. Reading it may make the paid run in D4 unnecessary.

## What it is

This proposal picks the token counting library and the coverage approach for the catalogue. It also bounds the sensitive-buffer claim.
It closes the two items M230_003 left open: live-provider count agreement and sensitive-buffer wiping ([evaluation](pr784-tokenizer-evaluation.md)).

## Why it exists

Nineteen of the 20 catalogue models have no local tokenizer, so they count one token per input byte.
`tiktoken_rs::tokenizer::get_tokenizer` resolves only `gpt-5.6-terra` (to `o200k_base`). Every Claude, GPT-6, Fireworks, OpenRouter, Groq, Mistral and Gemini entry returns `None`.
The fallback lives in `rustd/crates/afd_inference/src/budget.rs` (`input_tokens`).

Both hosts plan compaction from that estimate (`afr_agent/src/loop/context/estimate.rs`, `afd_authoring/src/model/context/estimate.rs`).
Measured against `o200k_base`, one token per byte overcounts typical input by 1.8× to 4.9×:

| Content | Bytes | `o200k_base` tokens | Bytes per token |
|---|---:|---:|---:|
| Prose | 9,500 | 1,951 | 4.87 |
| Rust source | 10,160 | 2,640 | 3.85 |
| Request JSON with tool calls | 12,702 | 3,896 | 3.26 |
| Logs with identifiers | 9,840 | 5,360 | 1.84 |

Compaction fires at 90 percent of the window as estimated (`afd_context/src/window.rs`, `COMPACT_PERCENT`), so a fleet compacts after roughly a fifth to half of its real window.
The three native routes (OpenAI Responses, Anthropic Messages, native Gemini) count natively only at dispatch, to size the output allowance, once the inflated estimate passes 80 percent (`afd_inference/src/count/mod.rs`, `CONFIRM_PERCENT`). The compaction planner never sees that count.

## How it behaves

### Library evaluation: `riptoken` 0.3.0 against `tiktoken-rs` 0.12.1

Both libraries read the same `.tiktoken` vocabularies and regex patterns. The harness fed `riptoken` the exact files `tiktoken-rs` bundles.
The corpus has 1,201 deterministic synthetic cases (1,048,309 bytes): prose, source, JSON, logs, combining marks, non-Latin scripts, emoji, control characters, literal special-token strings, and long whitespace and repeated-character runs.
OpenAI's Python `tiktoken` 0.12.0 is the reference.

| Encoding | `tiktoken-rs` matches reference | `riptoken` matches reference | `riptoken` count differences |
|---|---:|---:|---:|
| `cl100k_base` | 1,201 / 1,201 | 1,184 / 1,201 | 4 |
| `o200k_base` | 1,201 / 1,201 | 1,184 / 1,201 | 0 |

Every `riptoken` mismatch sits in a regex piece longer than 500 bytes, such as 3,000 spaces or 5,000 repeated letters.
That is `riptoken`'s `LARGE_PIECE_THRESHOLD` heap merge path, which picks a different merge order.
Long whitespace and repeated runs are ordinary in logs, indented source and minified data. This contradicts the crate's "byte-identical" claim.

Performance in Rust, median of seven samples, on 300,493 bytes of fresh input:

| Encoding | Cold build (ms) `tiktoken-rs` / `riptoken` | Single thread (ms) | Eight threads × 10 documents (ms) |
|---|---|---|---|
| `cl100k_base` | 39.2 / 36.4 | 16.95 / 14.30 | 54.18 / 38.21 |
| `o200k_base` | 63.7 / 81.1 | 15.62 / 15.46 | 41.20 / 33.05 |

The README's 2.4× to 6.2× figures compare against Python `tiktoken`. Against `tiktoken-rs` the gain is 1.0× to 1.2× single-threaded and 1.25× to 1.4× on eight threads.
`tiktoken-rs` already caches each encoder as a `&'static` singleton (`bpe_for_model`), so production pays the cold build once per process.

Other facts, read from the 0.3.0 source:

- MIT license, no `unsafe`, no prompt-piece cache. Its one `thread_local!` holds a thread index.
- `rayon` is a required dependency.
- The Rust crate bundles no vocabulary, so we would ship and pin the `.tiktoken` files ourselves.
- Eight releases between 2026-04-09 and 2026-04-12, with no release since.

The harness lives in `/private/tmp/riptoken-eval`: `cargo run --release` writes `run.log`, `DUMP=cl100k|o200k` writes per-case outputs, and the `resolve` and `ratio` binaries produce the tables above.
The reference check is `uv run --no-project --with tiktoken==0.12.0 python -I /private/tmp/riptoken-oracle/oracle.py <dir> <encoding>`.
This is evaluation evidence, not a repository verification result.

### Decisions requested

**D1: library.** Keep `tiktoken-rs`; reject `riptoken`. Correctness is a mandatory gate, and `riptoken` fails it on 1.4 percent of cases. Speed cannot make up for that, and the gain is small anyway. Reconsider only if a release fixes the long-piece path and passes this harness.

**D2: coverage for models with no public tokenizer.** Claude, GPT-6 and Gemini publish no tokenizer files. Several open-weight files carry redistribution limits; the MiniMax Community license is one ([evaluation](pr784-tokenizer-evaluation.md)). Exact local counting is impossible for most of the catalogue.

| Option | What a fleet gets | Cost |
|---|---|---|
| **A. `o200k_base` proxy plus a measured per-model multiplier (recommended)** | Estimates within a measured margin above the provider's count, so compaction happens near the real limit | One calibration run per catalogue change; the multiplier is a compiled table, checked against a committed report |
| B. Pinned exact tokenizer files where licenses allow | Exact counts for a few open models only | Asset pinning, license review and roughly 10 MB per file; Claude, GPT-6 and Gemini still uncovered |
| C. Keep one token per byte | Nothing changes: early compaction continues | None |

Option A's multiplier for each model is the largest provider-to-proxy ratio across every corpus category, plus headroom. That keeps the estimate an upper bound on the measured corpus.
Native counting stays the authority near the window on the three native routes.
`openai/gpt-oss-120b` gains exact `o200k_harmony` resolution once the provider prefix is stripped. `tiktoken-rs` already knows `gpt-oss-120b`.
Unknown and configured Ollama models keep one token per byte, which is a true upper bound for byte-level encodings apart from template framing.

**D3: what "buffer wiping" claims.** A Rust process cannot prove that every copy of conversation text is erased: `rig-core`, `reqwest`, `hyper`, `serde`, the Postgres driver and the allocator all keep their own copies. The proposed claim is narrow:

1. No prompt text is cached in the counting path. `tiktoken-rs` has no piece memoization; a test pins that, and any future tokenizer must disable its cache before first use.
2. Serialized request bodies, token-identifier vectors, summary text and compaction strings that `agentsfleet` code allocates are wrapped in `Zeroizing` and erased on drop.
3. No prompt, summary or credential text reaches traces, error messages or `Debug` output. Capture tests prove this.
4. Cross-tenant, private-draft and superseded-lease refusals are rechecked through real storage in `make test-integration-rustd`.
5. `docs/architecture/context_lifecycle.md` lists what stays unclaimed: third-party and allocator buffers, and a compromised host.

**D4: live calibration run.** This is a paid, opt-in run that never sits in the default lanes. The three native routes use their free counting endpoints. The twelve chat-route models need one generation per corpus case with `max_tokens: 1`, reading the reported input tokens.
Before sending anything, the run prints the expected spend from catalogue prices, and you approve the run. Credentials come from the environment-selected vault, and a missing one stops the run by name.
The corpus is synthetic only; no account conversation text is sent.
After it lands, the existing content-free `model_input_calibration` trace becomes the production check on drift.

**Context rejection.** A provider context-length rejection gets at most one fresh recount and replan, and no retry after an unknown charged outcome. The evaluation lists this as open, and it is the safety net under Option A.

## After sign-off

A spec turns the remaining options into Sections: per-model counting profiles, the calibration source, context-rejection recovery, and private-input lifetime. Each Dimension maps to one test.
It takes the next free milestone number; M233_001 is already the chat round-trip stage-timing spec.
The spec is reviewed for completeness before it leaves `pending/`.
