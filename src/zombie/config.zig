// Zombie configuration façade — re-exports types + parse/validate/markdown
// entry points so consumers keep a single import point (`config.X`).
//
// Directory-based zombie format (SKILL.md + TRIGGER.md). `zombiectl install
// --from <path>` sends both files raw. The server parses TRIGGER.md frontmatter
// into config_json via parseTriggerMarkdownWithJson (see config_markdown.zig).
// SKILL.md is stored as-is.
// At claim time, the worker calls:
//   - parseZombieConfig(alloc, config_json_bytes)  → ZombieConfig struct
//   - extractZombieInstructions(source_markdown)    → system prompt slice (borrowed)
//
// Implementation lives in:
//   - config_types.zig     — value types + destructors
//   - config_parser.zig    — JSON → ZombieConfig, per-field helpers
//   - config_markdown.zig  — TRIGGER.md frontmatter extraction
//   - config_validate.zig  — tool / credential registry checks
//   - config_helpers.zig   — shared parse sub-routines (trigger, network, budget)
//   - config_gates.zig     — gate/anomaly policy types + parser

const config_types = @import("config_types.zig");
const config_parser = @import("config_parser.zig");
const config_markdown = @import("config_markdown.zig");
const config_validate = @import("config_validate.zig");
const config_gates = @import("config_gates.zig");

// Value types.
pub const ZombieConfigError = config_types.ZombieConfigError;
pub const ZombieStatus = config_types.ZombieStatus;
pub const ZombieTriggerType = config_types.ZombieTriggerType;
pub const ZombieTrigger = config_types.ZombieTrigger;
pub const WebhookSignatureConfig = config_types.WebhookSignatureConfig;
pub const MAX_SIGNATURE_HEADER_LEN = config_types.MAX_SIGNATURE_HEADER_LEN;
pub const ZombieBudget = config_types.ZombieBudget;
pub const ZombieNetwork = config_types.ZombieNetwork;
pub const ZombieConfig = config_types.ZombieConfig;

// Gate/anomaly policy types (owned by config_gates, surfaced here for callers).
pub const GateBehavior = config_gates.GateBehavior;
pub const GateRule = config_gates.GateRule;
pub const AnomalyPattern = config_gates.AnomalyPattern;
pub const AnomalyRule = config_gates.AnomalyRule;
pub const GatePolicy = config_gates.GatePolicy;

// Entry points.
pub const parseZombieConfig = config_parser.parseZombieConfig;
pub const parseZombieFromTriggerMarkdown = config_markdown.parseZombieFromTriggerMarkdown;
pub const parseTriggerMarkdownWithJson = config_markdown.parseTriggerMarkdownWithJson;
pub const ParsedTrigger = config_markdown.ParsedTrigger;
pub const extractZombieInstructions = config_markdown.extractZombieInstructions;

// Test discovery — Zig only runs tests in transitively imported files. The
// implementation modules are already reached via the `const` imports above,
// but test files contain no `pub` symbols the façade consumes, so pull them
// in explicitly here. test {} blocks are stripped in release builds, so
// this adds zero bytes to production binaries. main.zig imports config.zig
// once; config.zig fans out to the implementation + test modules.
test {
    _ = @import("config_helpers.zig"); // has inline tests, no other fanout path
    _ = @import("config_types_test.zig");
    _ = @import("config_parser_test.zig");
    _ = @import("config_markdown_test.zig");
    _ = @import("config_validate_test.zig");
}
