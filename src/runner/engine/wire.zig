//! Engine wire-schema field names.
//!
//! Single source of truth for the JSON keys the engine reads out of the lease
//! payload by hand — the `ExecutionPolicy` / CreateExecution params and the
//! `agent_config` child fields — dereferenced as `wire.X` in
//! `context_budget.zig` (fromJson), `runner_helpers.zig` (applyAgentConfig),
//! `runner.zig`, and `child_exec.zig`. The result frame and correlation
//! identity fields are (de)serialized by std.json struct reflection over
//! `ExecutionResult` / `CorrelationContext`, so their JSON keys come from the
//! Zig field identifiers — not from this file.
//!
//! The pipe framing itself (`[type][len][payload]`) lives in
//! `runner/pipe_proto.zig`; the `/v1/runners` wire types live in `protocol.zig`.

// ── ExecutionPolicy (CreateExecution params) ────────────────────────────
pub const network_policy = "network_policy";
pub const allow = "allow";
pub const tools = "tools";
pub const secrets_map = "secrets_map";
pub const context = "context";
pub const tool_window = "tool_window";
pub const memory_checkpoint_every = "memory_checkpoint_every";
pub const stage_chunk_threshold = "stage_chunk_threshold";
pub const model = "model";
pub const context_cap_tokens = "context_cap_tokens";

// ── StartStage payload + agent_config children ──────────────────────────
pub const provider = "provider";
pub const temperature = "temperature";
pub const max_tokens = "max_tokens";
pub const api_key = "api_key";
pub const message = "message";
