// Top-level help tail for the agentsfleet program — the journey-oriented
// command guide plus the environment-variable matrix that commander's
// default help body omits. Kept in its own file so the guide can grow
// without pressing cli-tree.ts against the LENGTH GATE (mirrors the
// cli-tree-agent.ts / cli-tree-memory.ts split). `addHelpText("after", …)`
// appends this verbatim; commander still owns the layout above it.
//
// Commander renders a flat alphabetical "Commands:" block that mixes
// nouns (workspace, agent-key) with the imperative agent verbs
// (install, list, status, …). That tells an operator *what exists* but
// not *what to run first*. This guide reorders the same commands by task
// so a new operator reads top-to-bottom: authenticate, pick a workspace,
// drive agents, then the occasional vault / key / billing / memory verb.

const HELP_WIDTH = 80;
const TITLE_INDENT = "  ";
const COMMAND_INDENT = "    ";
const COMMAND_GUTTER = "  ";

interface CommandGroup {
  readonly title: string;
  readonly commands: readonly string[];
}

interface EnvVar {
  readonly name: string;
  readonly desc: string;
}

// Journey order, not alphabetical. The imperative verbs (install, list,
// status, …) are TOP-LEVEL commands; `agent update` is the lone member of
// the `agent` group. This guide is the only place that split is spelled
// out, so `agentsfleet agent list` confusion resolves by reading here.
const COMMAND_GUIDE: readonly CommandGroup[] = [
  { title: "Setup", commands: ["login", "logout", "auth status", "doctor"] },
  {
    title: "Workspaces",
    commands: [
      "workspace add", "workspace list", "workspace use",
      "workspace show", "workspace credentials", "workspace delete",
    ],
  },
  {
    title: "Agents",
    commands: [
      "install", "list", "status", "steer", "logs", "events",
      "stop", "resume", "kill", "delete", "agent update",
    ],
  },
  {
    title: "Workspace credentials",
    commands: ["credential add", "credential show", "credential list", "credential delete"],
  },
  { title: "Agent keys", commands: ["agent-key add", "agent-key list", "agent-key delete"] },
  { title: "Integration grants", commands: ["grant list", "grant delete"] },
  {
    title: "Tenant & billing",
    commands: ["tenant provider show", "tenant provider add", "tenant provider delete", "billing show"],
  },
  { title: "Memory", commands: ["memory list", "memory search"] },
];

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

// Render a group as a title line followed by its commands packed two-space-
// indented, wrapping before HELP_WIDTH so the guide never assumes a wide
// terminal (the 80-column help invariant the golden test pins).
function renderGroup(group: CommandGroup): string[] {
  const lines = [TITLE_INDENT + group.title];
  let row = "";
  for (const command of group.commands) {
    const candidate = row === "" ? command : `${row}${COMMAND_GUTTER}${command}`;
    // Flush only when there's a packed row to break before this command — an
    // empty row means `command` is the first/sole token, so accept it rather
    // than emitting a stray indented blank line ahead of it.
    if (row !== "" && COMMAND_INDENT.length + candidate.length > HELP_WIDTH) {
      lines.push(COMMAND_INDENT + row);
      row = command;
    } else {
      row = candidate;
    }
  }
  if (row !== "") lines.push(COMMAND_INDENT + row);
  return lines;
}

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
    "Commands by task:",
    ...COMMAND_GUIDE.flatMap(renderGroup),
    "",
    "Environment variables:",
    ...renderEnvVars(),
  ].join("\n");
}
