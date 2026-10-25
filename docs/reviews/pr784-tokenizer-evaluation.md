# Token counting decision rubric

Updated: 2026-10-25. The tokenizer comparison below is an isolated evaluation.

Native counting is wired into both production hosts; shared, runtime and real-storage boundary tests pass. Live-provider accuracy and complete sensitive-buffer wiping remain open.

**Decision:** Keep `tiktoken-rs` for verified OpenAI encodings. Prefer pinned Hugging Face tokenizers for new local model coverage. Retain Kitoken as a comparison candidate.

Keep experimental `tiktoken` out of production pending its model mappings and private-input cache requirements.

## Rubric

Correctness and private-input retention are mandatory gates. Performance cannot compensate for either failure. “Unverified” remains open; it never counts as a pass.

| Criterion | Required evidence | `tiktoken-rs` 0.12.1 | `tiktoken` 4.1.2 | `tokenizers` 0.23.1 | Kitoken 0.11.0 |
|---|---|---|---|---|---|
| Reference token agreement | Zero token-identifier and count differences for each enabled model/revision | OpenAI comparison baseline | Pass: both tested OpenAI encodings. Fail: MiniMax M2 is incompatible with the tested M3.1 file. | MiniMax M3.1 reference reader; independent Kitoken agreement | Pass: MiniMax M3.1 comparison |
| Exact supported-model mapping | Explicit model, provider, vocabulary and revision; no family inference alone | Only one current catalogue name resolves | Five names resolve; one selects the incompatible MiniMax encoding pipeline | Requires pinned files and explicit mappings | Requires pinned files and explicit mappings |
| Complete request count | Roles, templates, tools, generation prefix and media included | Text tokenizer; additional request logic required | Text tokenizer; additional request logic required | Vocabulary alone is insufficient | Vocabulary alone is insufficient |
| Retained private input | No long-lived prompt-piece cache enabled | No prompt-piece memoization in inspected implementation | Fail for proposed shared use: thread-local raw pieces, no public disable/erase option found | Conditional: disable input caching before first use; clearing only changes cache generation | No input memoization found in inspected source; full buffer wiping remains unproved |
| Performance | Separate cold/warm, repeated/novel input and concurrent measurements | Measured baseline below | Measured below; repeated-piece cache affects results | Measured below with input cache disabled | Measured below |
| Asset identity and license | Pinned revision, verified checksum, approved distribution terms | Bundled encodings | Bundled encodings; exact model compatibility still required | Model file license review required before distribution | Same model file requirement |
| Production admission | All applicable gates pass in the real request path | Retain existing coverage; fix request approximation | Hold production adoption | Preferred next implementation candidate | Keep as challenger |

The current catalogue has 20 hosted entries. A successful library name lookup does not establish a correct request count.
The three GPT-6 entries resolve in neither tested Tiktoken implementation.

The experimental crate resolves the OpenRouter MiniMax M3.1 name to `minimax_m2`.
That mapping must not enter admission merely because lookup succeeds.

## Measured evidence

The isolated Rust experiment uses 1,036 synthetic strings: prose, source, JSON, logs, combining marks, non-Latin scripts, emoji, control characters and literal special-token strings.
It sends no inference requests and uses no account conversation text.

| Comparison | Cases | Token-identifier differences | Count differences |
|---|---:|---:|---:|
| Experimental `cl100k_base` versus installed `tiktoken-rs` | 1,036 | 0 | 0 |
| Experimental `o200k_base` versus installed `tiktoken-rs` | 1,036 | 0 | 0 |
| Experimental `minimax_m2` versus pinned MiniMax M3.1 through Hugging Face | 1,036 | 344 | 344 |
| Kitoken versus the same MiniMax M3.1 file through Hugging Face | 1,036 | 0 | 0 |

These results establish agreement on this corpus only. Neither library is a universal oracle.
The MiniMax result rejects the M2 substitution; it does not reject the experimental crate's supported M2 implementation.

The comparison covers the full encoding pipeline, including normalization; it does not isolate the vocabulary as the cause.

Ten encodings of the same 90,741-byte corpus took 57 ms for installed `cl100k_base` and 35 ms for installed `o200k_base`.
Experimental count-only calls took 4 ms for each encoding; first construction took 11 ms and 25 ms respectively.
These compare the current allocating call with a count-only alternative, under repeated input.

They do not establish production latency, fresh-input throughput, memory use or a general speed ratio.

A second experiment separates repeated input from fresh deterministic random words, with a small Unicode and source footer.
Each single-thread sample counts ten documents: 370,030 bytes for repeated input, or 306,710 bytes for fresh input.
The table reports medians of seven samples, in milliseconds; cold construction has one observation per implementation.

| Implementation and encoding | Cold construction | Repeated input | Fresh input | Eight workers, fresh input |
|---|---:|---:|---:|---:|
| `tiktoken-rs`, `cl100k_base` | 39.036 | 15.892 | 14.245 | 35.206 |
| `tiktoken`, `cl100k_base` | 11.496 | 1.604 | 9.271 | 16.304 |
| `tiktoken-rs`, `o200k_base` | 53.602 | 8.010 | 13.410 | 27.534 |
| `tiktoken`, `o200k_base` | 22.981 | 1.280 | 10.179 | 17.171 |
| Hugging Face, MiniMax M3.1, cache disabled | 173.979 | 69.018 | 56.759 | 111.569 |
| Kitoken, MiniMax M3.1 | 219.342 | 14.678 | 17.949 | 32.680 |

The eight-worker sample counts eight copies of the ten fresh documents, with a distinct worker prefix.
Its duration includes thread startup and prefix allocation; it performs eight times the single-thread work.
These measurements do not establish production memory use, complete-request latency, or a latency admission threshold.

Reproduce the initial experiment with the saved Cargo manifest, lockfile, corpus and source under `/private/tmp/pr784-tokenizer-evaluation`.
Run `cargo run --release --bin pr784-tokenizer-evaluation` there; the captured output is `run.log`.
The separate timing command is `cargo run --release --bin performance`; its corrected byte counts and results are in `performance-fresh-final.log`.

This isolated command is evaluation evidence, not a repository verification result.

The reference file is [MiniMax M3.1 Flash `tokenizer.json`](https://huggingface.co/MiniMaxAI/MiniMax-M3.1-Flash/blob/54e4e69dfc452b1e94a426c24a4fdeeaa8e3ad5e/tokenizer.json).
Revision: `54e4e69dfc452b1e94a426c24a4fdeeaa8e3ad5e`.
SHA-256: `bb1f1626cf01448f1e3b6036d0a061ffc66c91d9046aada14ea23a5441b5ad6e`; size: 9,731,500 bytes.

No model weights or remote model code were loaded. The model repository uses the MiniMax Community license; files are not redistributed here.

## Provider-aware decision

| Supported route | Counting direction | Remaining proof |
|---|---|---|
| OpenAI Responses | Verified local encoding, then native input count for uncertain or complex requests near the window | Exact GPT-6 mapping; full prepared-request confirmation |
| Anthropic Messages | Cheap explicit estimate, then native message counting near the window | Live-provider comparison; unsupported native inputs refuse locally |
| Native Gemini | Native `countTokens` with complete `generateContentRequest` when confirmation is required | System instructions, tools, media and reported-usage comparison |
| Fireworks, Groq and Mistral | Exact model vocabulary and template when verified | Current deployed model revisions and provider-added framing |
| OpenRouter | Count for the selected model and configured route | Actual routing identity and template; no cross-provider prompt forwarding |
| Configured Ollama | Verify the server model's vocabulary/template identity | Custom models, mutable tags and server configuration changes |

Native counts run outside database transactions. A returned count binds to the same prepared body, route, model and tokenizer/template revision.

Recheck freshness before dispatch. A failed count never becomes zero.

Near-window confirmation must not introduce a network call before every ordinary turn.

Official references: [OpenAI input counting](https://developers.openai.com/api/docs/guides/token-counting), [Anthropic message counting](https://platform.claude.com/docs/en/build-with-claude/token-counting), and [Gemini countTokens](https://ai.google.dev/api/tokens).

Anthropic documents count inputs it cannot accept, including most server tools and URL/file-backed media.
Its count remains an estimate.
The native adapter refuses external media, server tools except advisor, and Model Context Protocol (MCP) connectors before sending a count request.

## Required test matrix

| Test class | Required cases | Status |
|---|---|---|
| Local tokenizer agreement | Pinned reference, Unicode, source, logs, JSON, whitespace and literal special tokens | Initial corpus measured above |
| Catalogue admission | Every entry from `scripts/model-library-allowlist.json`; unknown and configured Ollama models | Catalogue corpus and output-admission tests pass in the shared crate |
| Request structure | System instructions, roles, tool schemas, results and opaque replay | Physical transport tests compare the selected input fields for all three native APIs, including tools, results and reasoning replay |
| Model change | Model/provider/dialect/template/revision changes force remeasurement | Request/model/route/window/output freshness tests pass; expanded template/media coverage remains open |
| Media | Images, Portable Document Format (PDF) files and base64 use native confirmation or an explicit modality estimate | Selected inline image/PDF fields reach all three native count requests; Anthropic URL/file inputs refuse locally; live-provider comparison open |
| Output allowance | Visible output and reasoning share finite reserved capacity; irreducible input refuses before dispatch | Existing window tests; provider comparison open |
| Native failures | Cancellation, timeout, rate limit, malformed count, unsupported input and changed request | Shared and runtime refusal tests pass. Authoring integration proves source/Stop/credential changes during count prevent paid dispatch; unavailable counts leave no paid intent, usage or checkpoint. |
| Context rejection | One fresh recount/replan maximum; no retry after an unknown charged outcome | Open |
| Calibration | Compare estimated and actual input by model, provider, revision and request kind; store no prompt text | Shared and runtime traces compare authoritative complete/partial usage without prompt, key or URL fields; absent usage emits no fabricated zero. Live-provider accuracy remains open. |
| Sensitive lifetime | Disable prompt memoization; inspect temporary and serialized copies; preserve tenant/lease authorization | Partial source review; wiping and storage proofs remain open |

Boundary evidence: shared inference 71 tests in `/private/tmp/pr784-count-shared-final-2.log`; runtime confirmation and calibration in the Rust portion of `/private/tmp/pr784-resumed-test-unit-all-3.log`.
Canonical `make test-integration-rustd` passed 1,397 tests, including all three authoring native-count cases, in `/private/tmp/pr784-resumed-test-integration-final.log`.
These controlled-provider and real-storage results do not establish live-provider token agreement.

## Custom compaction logic

| Custom code | Responsibility | Decision |
|---|---|---|
| `Window::{needs_compaction,fit_output}` | Trigger, safety margin and finite output allowance | Keep and test boundary arithmetic |
| `prepare`, `Reading::append`, `compact` | Ordered source reads, prefix verification, whole-group selection and geometric sizing | Keep; application policy |
| Runtime `Index::refresh_with` | Group messages/tool results and refresh the mutable suffix | Keep; preserve atomic history and linear work |
| `SUMMARY_INSTRUCTIONS`, `Summary::{decode,context}` | Fixed model instructions, bounded JSON and untrusted restoration | Keep; add semantic quality evaluation |
| `Checkpoint::{belongs_to,verify_source}`, `digest_groups` | Scope/model/state binding and covered-source verification | Keep; hashing uses `sha2` |
| `Compaction::finish_with`, `execute` | Consume preparation, summarize, validate and conditionally publish | Keep; storage guards remain mandatory |
| Authoring `Snapshot::verify` | Recheck history and authority after funding | Keep; reject stale prepared work |
| `input_tokens`, `Configured::measure_input` | Model lookup, byte fallback, serialized request tokenization and per-message allowance | Replace unverified approximations through the counting follow-up |

Summary prose comes from the selected model. JSON parsing uses `serde_json`; bounds use `garde`.

No custom tokenizer or cryptographic primitive generates the summary.
The custom request approximation is one byte per unknown-model input byte, or serialized-JSON tokenization plus 32 per history message.

Neither is a proven upper bound on the provider's formatted input.

Library sources: [experimental encoding coverage](https://docs.rs/tiktoken/4.1.2/tiktoken/), [experimental input cache](https://github.com/goliajp/rust-tiktoken/blob/v4.1.2/tiktoken/src/piece_cache.rs), [Hugging Face BPE cache controls](https://docs.rs/tokenizers/0.23.1/tokenizers/models/bpe/struct.BPE.html), and [Kitoken](https://github.com/Systemcluster/kitoken).
