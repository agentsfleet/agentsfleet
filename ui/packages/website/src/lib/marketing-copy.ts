// No Vite-only syntax here (?url, CSS imports, import.meta.env):
// tests/e2e/*.spec.ts load this through Playwright's transform and
// scripts/prebuild.mjs through Bun, and neither runs Vite plugins.
export const PRODUCT_NAME = "agentsfleet";

export const HERO_HEADLINE = "What should your fleet handle?";

const PILLAR_TRIGGER = "wakes on a production event";
const PILLAR_SOURCES = "logs, metrics, and code";
const PILLAR_CONTROL = "You control access and decide what ships.";

// Product context shared with the machine-readable website summary.
export const PILLAR_TOKENS = [PILLAR_TRIGGER, PILLAR_SOURCES, PILLAR_CONTROL] as const;

// Trigger and scope terminology used by the positioning checks.
export const HERO_LEDE_PARTS = {
  intro: "An agent",
  trigger: PILLAR_TRIGGER,
  middle: "and investigates with your",
  sources: PILLAR_SOURCES,
  outro: `to explain what went wrong and, when a fix is warranted, prepare one. ${PILLAR_CONTROL}`,
} as const;

export const HERO_PRIMARY_LABEL = "Join waitlist";
export const HERO_SECONDARY_LABEL = "See how it works";
export const HOW_IT_WORKS_ANCHOR_ID = "how-it-works";
export const LOOP_ANCHOR_ID = HOW_IT_WORKS_ANCHOR_ID;

export type SourceCategory = {
  id: string;
  label: string;
  icon: string;
  examples: readonly string[];
};

export const SOURCE_CATEGORIES: readonly SourceCategory[] = [
  {
    id: "signals",
    label: "Signals",
    icon: "/logos/signals.svg",
    examples: ["ticket escalation", "workflow_run", "cron", "manual steer"],
  },
  {
    id: "telemetry",
    label: "Telemetry",
    icon: "/logos/telemetry.svg",
    examples: ["logs", "traces", "metrics", "run history"],
  },
  {
    id: "code",
    label: "Code",
    icon: "/logos/code.svg",
    examples: ["repository", "tests", "pull requests", "recent deploys"],
  },
  {
    id: "control-plane",
    label: "Control plane",
    icon: "/logos/control-plane.svg",
    examples: ["approvals", "vault", "policy", "audit trail"],
  },
] as const;

// Machine-readable explanation of the same incident example shown on the homepage.
export const LOOP_STEPS = [
  { number: "01", title: "Configure the job once", description: "Describe a job in workspace chat, review its trigger, tools, resources, credentials, and access, then Save to create a fleet. Publish to the Library separately when you want to reuse a version." },
  { number: "02", title: "Watch the investigation", description: "A scheduled sweep wakes the incident responder. It reads Grafana Loki and GitHub evidence and returns a diagnosis with references." },
  { number: "03", title: "Act on the report", description: "The investigator can report to configured Slack and Jira destinations. It cannot write repository code. A separately configured repairer with an approved write grant can open a draft pull request; a human reviews and merges." },
] as const;

export const PRICING_COPY = {
  headline: "Bring a real workflow. Put your fleet to work.",
  lede: "Investigate an incident, catch a test regression, or automate recurring work. Create a fleet around the tools and access you choose.",
  status: "$0 to start · $25 credit included",
  note: "If you bring your own key, usage is billed by your provider directly.",
  runtime: "Fleet runtime starts at $0.",
  models: "Run your first jobs with $25 in included credit.",
} as const;
