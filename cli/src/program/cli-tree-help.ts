// Top-level help tail for the agentsfleet program — the environment-variable
// matrix that commander's default help body omits. Kept in its own file so it
// can grow without pressing cli-tree.ts against the LENGTH GATE (mirrors the
// cli-tree-agent.ts / cli-tree-memory.ts split). `addHelpText("after", …)`
// appends this verbatim; commander still owns the layout above it.

const TITLE_INDENT = "  ";
const COMMAND_GUTTER = "  ";

interface EnvVar {
  readonly name: string;
  readonly desc: string;
}

const ENV_VARS: readonly EnvVar[] = [
  { name: "AGENTSFLEET_API_URL", desc: "API base URL (overridden by --api)" },
  { name: "AGENTSFLEET_DASHBOARD_URL", desc: "Dashboard base URL (login verify page)" },
  { name: "AGENTSFLEET_API_KEY", desc: "Service API key (overrides stored login)" },
  { name: "AGENTSFLEET_STATE_DIR", desc: "Directory for local CLI state files" },
  { name: "NO_COLOR", desc: "Any non-empty value disables color" },
  { name: "AGENTSFLEET_TELEMETRY_DISABLED", desc: "Set to 1 to opt out of analytics+tracing" },
  { name: "DO_NOT_TRACK", desc: "Industry-standard opt-out signal" },
  { name: "AGENTSFLEET_TELEMETRY_POSTHOG_KEY", desc: "Override the PostHog project key" },
  { name: "AGENTSFLEET_TELEMETRY_POSTHOG_HOST", desc: "Override the PostHog ingest host" },
  { name: "AGENTSFLEET_TELEMETRY_DEBUG", desc: "Set to 1 to log span summaries to stderr" },
];

// Pad every name to one common column so descriptions align in a single
// gutter — the previous hand-spaced rows drifted because short names
// (NO_COLOR, DO_NOT_TRACK) used a narrower pad than the AGENTSFLEET_* rows.
function renderEnvVars(): string[] {
  const column = Math.max(...ENV_VARS.map((v) => v.name.length)) + COMMAND_GUTTER.length;
  return ENV_VARS.map((v) => `${TITLE_INDENT}${v.name.padEnd(column)}${v.desc}`);
}

export function helpTail(): string {
  return [
    "",
    "Environment variables:",
    ...renderEnvVars(),
  ].join("\n");
}
