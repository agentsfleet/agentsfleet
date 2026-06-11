//! Engine wire-schema field names.
//!
//! Single source of truth for the JSON keys the engine reads out of the lease
//! payload by hand — the `ExecutionPolicy` / CreateExecution params and the
//! `agent_config` child fields — dereferenced as `wire.X` in
//! `runner_helpers.zig` (applyAgentConfig), `runner.zig`, and `child_exec.zig`. The result frame and correlation
//! identity fields are (de)serialized by std.json struct reflection over
//! `ExecutionResult`, so their JSON keys come from the
//! Zig field identifiers — not from this file.
//!
//! The pipe framing itself (`[type][len][payload]`) lives in
//! `runner/pipe_proto.zig`; the `/v1/runners` wire types live in `protocol.zig`.

pub const model = "model";

// ── StartStage payload + agent_config children ──────────────────────────
pub const provider = "provider";
pub const temperature = "temperature";
pub const max_tokens = "max_tokens";
pub const api_key = "api_key";
pub const inference_host = "inference_host";
pub const message = "message";

// ── Reasoning context (composeMessage) ──────────────────────────────────────
/// Context key carrying the installed agent's `SKILL.md` body so the child's
/// `composeMessage` renders it ahead of the trigger event. Soft reasoning input
/// — never a secret; written by `child_exec`, read by `runner_helpers`.
pub const installed_instructions = "installed_instructions";
