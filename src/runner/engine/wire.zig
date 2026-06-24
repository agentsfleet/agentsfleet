//! Engine wire-schema field names.
//!
//! Single source of truth for the JSON keys the engine reads out of the lease
//! payload by hand — the `ExecutionPolicy` / CreateExecution params and the
//! `fleet_config` child fields — dereferenced as `wire.X` in
//! `runner_helpers.zig` (applyFleetConfig), `runner.zig`, and `child_exec.zig`. The result frame and correlation
//! identity fields are (de)serialized by std.json struct reflection over
//! `ExecutionResult`, so their JSON keys come from the
//! Zig field identifiers — not from this file.
//!
//! The pipe framing itself (`[type][len][payload]`) lives in
//! `runner/pipe_proto.zig`; the `/v1/runners` wire types live in `protocol.zig`.

pub const model = "model";

// ── StartStage payload + fleet_config children ──────────────────────────
pub const provider = "provider";
pub const temperature = "temperature";
pub const max_tokens = "max_tokens";
pub const api_key = "api_key";
pub const inference_host = "inference_host";
/// Custom OpenAI-compatible endpoint URL the engine must dial — set on the
/// `fleet_config` only for a `custom:<url>` provider; the runner copies it onto
/// the nullclaw `ProviderEntry.base_url` so the request reaches the custom host.
pub const base_url = "base_url";
pub const message = "message";

// ── Reasoning context (composeMessage) ──────────────────────────────────────
/// Context key carrying the installed fleet's `SKILL.md` body so the child's
/// `composeMessage` renders it ahead of the trigger event. Soft reasoning input
/// — never a secret; written by `child_exec`, read by `runner_helpers`.
pub const installed_instructions = "installed_instructions";
