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

export const HERO_PRIMARY_LABEL = "Join waitlist";
export const LOOP_ANCHOR_ID = "how-it-works";

export type SourceCategory = {
  label: string;
  examples: readonly string[];
};

export const SOURCE_CATEGORIES: readonly SourceCategory[] = [
  {
    label: "Signals",
    examples: ["ticket escalation", "workflow_run", "cron", "manual steer"],
  },
  {
    label: "Telemetry",
    examples: ["logs", "traces", "metrics", "run history"],
  },
  {
    label: "Code",
    examples: ["repository", "tests", "pull requests", "recent deploys"],
  },
  {
    label: "Control plane",
    examples: ["approvals", "vault", "policy", "audit trail"],
  },
] as const;

// Machine-readable explanation of the same incident example shown on the homepage.
export const LOOP_STEPS = [
  { number: "01", title: "Find the delay", description: "Leah reports a frozen chat in Slack. Mira asks Scout to check reply times and connection errors in Grafana." },
  { number: "02", title: "Review the fix", description: "A repair fleet with an approved write grant opens a draft pull request with a test. Leah reviews and ships the change." },
  { number: "03", title: "Check the result", description: "Scout compares an hour before the fix with an hour after it. Mira reports the change in reply times and packet loss in Leah’s thread." },
] as const;

export const PRICING_COPY = {
  headline: "Bring a real workflow. Put your fleet to work.",
  lede: "Investigate an incident, catch a test regression, or automate recurring work. Create a fleet around the tools and access you choose.",
  status: "$0 to start",
  note: "If you bring your own key, usage is billed by your provider directly.",
  runtime: "Fleet runtime starts at $0.",
} as const;
