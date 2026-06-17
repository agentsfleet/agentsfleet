// Agent configuration façade — re-exports types + parse/validate/markdown
// entry points so consumers keep a single import point (`config.X`).
//
// Directory-based agent format (SKILL.md + TRIGGER.md). `agentsfleet install
// --from <path>` sends both files raw. The server parses TRIGGER.md frontmatter
// into config_json via parseTriggerMarkdownWithJson (see config_markdown.zig).
// SKILL.md is stored as-is.
// At claim time, the worker calls:
//   - parseAgentConfig(alloc, config_json_bytes)  → AgentConfig struct
//   - extractAgentInstructions(source_markdown)    → system prompt slice (borrowed)
//
// Implementation lives in:
//   - config_types.zig     — value types + destructors
//   - config_parser.zig    — JSON → AgentConfig, per-field helpers
//   - config_markdown.zig  — TRIGGER.md frontmatter extraction
//   - config_validate.zig  — tool / credential registry checks
//   - config_helpers.zig   — shared parse sub-routines (trigger, network, budget)
//   - config_gates.zig     — gate/anomaly policy types + parser

const config_types = @import("config_types.zig");
const config_parser = @import("config_parser.zig");
const config_markdown = @import("config_markdown.zig");
const config_gates = @import("config_gates.zig");

// Value types.
pub const AgentConfigError = config_types.AgentConfigError;
pub const AgentStatus = config_types.AgentStatus;
pub const AgentTrigger = config_types.AgentTrigger;
pub const WebhookSignatureConfig = config_types.WebhookSignatureConfig;
pub const MAX_SIGNATURE_HEADER_LEN = config_types.MAX_SIGNATURE_HEADER_LEN;
pub const AgentBudget = config_types.AgentBudget;
pub const AgentNetwork = config_types.AgentNetwork;
pub const AgentConfig = config_types.AgentConfig;
pub const SkillMetadata = config_types.SkillMetadata;
pub const validRequiredTags = config_types.validRequiredTags;

// Gate/anomaly policy types (owned by config_gates, surfaced here for callers).
pub const GatePolicy = config_gates.GatePolicy;
// Write-time gate-condition validation (UZ-APPROVAL-005); runtime parse stays lenient.
pub const firstInvalidGateCondition = config_gates.firstInvalidCondition;

// Entry points.
pub const parseAgentConfig = config_parser.parseAgentConfig;
pub const parseTriggerMarkdownWithJson = config_markdown.parseTriggerMarkdownWithJson;
pub const parseSkillMetadata = config_markdown.parseSkillMetadata;
pub const ParsedTrigger = config_markdown.ParsedTrigger;
pub const extractAgentInstructions = config_markdown.extractAgentInstructions;

// Test discovery — Zig only runs tests in transitively imported files. The
// implementation modules are already reached via the `const` imports above,
// but test files contain no `pub` symbols the façade consumes, so pull them
// in explicitly here. test {} blocks are stripped in release builds, so
// this adds zero bytes to production binaries. main.zig imports config.zig
// once; config.zig fans out to the implementation + test modules.
test {
    _ = @import("config_helpers_test.zig");
    _ = @import("config_types_test.zig");
    _ = @import("config_parser_test.zig");
    _ = @import("config_markdown_test.zig");
    _ = @import("config_validate_test.zig");
    _ = @import("frontmatter_fixtures_test.zig");
}
