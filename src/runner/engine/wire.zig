//! Executor RPC wire-schema field names.
//!
//! Single source of truth for every JSON object key the worker writes
//! (via `client.zig`) and the executor sidecar parses (via `handler.zig`
//! + `context_budget.zig`). One declaration per field — RULE UFS at the
//! protocol level. Adding a new field means adding it here first; both
//! sides reference these constants instead of repeating string literals.
//!
//! JSON-RPC envelope fields (`id`, `result`, `error`, `code`, `message`
//! when used as RPC error message) are protocol-level and live in
//! `protocol.zig` next to `Method` and `ErrorCode`.

// ── Identity / correlation ──────────────────────────────────────────────
pub const workspace_path = "workspace_path";
pub const trace_id = "trace_id";
pub const zombie_id = "zombie_id";
pub const workspace_id = "workspace_id";
pub const session_id = "session_id";
pub const execution_id = "execution_id";

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
pub const agent_config = "agent_config";
pub const provider = "provider";
pub const system_prompt = "system_prompt";
pub const temperature = "temperature";
pub const max_tokens = "max_tokens";
pub const api_key = "api_key";
pub const message = "message";
pub const memory_connection = "memory_connection";
pub const memory_namespace = "memory_namespace";

// ── Response shape ──────────────────────────────────────────────────────
pub const content = "content";
pub const token_count = "token_count";
pub const wall_seconds = "wall_seconds";
pub const exit_ok = "exit_ok";
pub const memory_peak_bytes = "memory_peak_bytes";
pub const cpu_throttled_ms = "cpu_throttled_ms";
pub const time_to_first_token_ms = "time_to_first_token_ms";
pub const memory_limit_bytes = "memory_limit_bytes";
pub const checkpoint_id = "checkpoint_id";
