//! Single source of truth for the synthetic-secret literal used by the
//! redaction-harness tests. Imported by `test_stub_provider.zig` (which
//! emits the canary in its canned ChatResponse) and by the worker-side
//! redaction tests (which assert the canary never reaches the wire).
//!
//! The literal is distinctive enough that `!recorder.contains(...)` will
//! not false-positive on coincidental byte sequences in JSON noise.

pub const SYNTHETIC_SECRET = "ZMBSTUB-redaction-canary-9c8f4e1a2d";

/// Placeholder the redactor substitutes for `agent_config.api_key` bytes.
/// Mirrors `collectSecrets` in `runner.zig` — keep these two in lock-step.
pub const PLACEHOLDER = "${secrets.llm.api_key}";
