export const PRODUCT_NAME = "agentsfleet";

export const HERO_HEADLINE = "A resident engineer that compounds operational knowledge.";

export const PILLAR_TOKENS = [
  "resident engineer",
  "human approval",
  "replayable log",
  "wake.on.event",
] as const;

export const HERO_LEDE_PARTS = {
  intro: "It wakes on the first signal, captures the",
  problemClass: "problem class",
  middle: "writes a scenario, a test, and a fix pull request, then holds at",
  humanApproval: "human approval",
  outro: "Everything it does is a",
  replayableLog: "replayable log",
  close: "you can audit line by line.",
} as const;

export const HERO_PRIMARY_LABEL = "Get early access";
export const HERO_SECONDARY_LABEL = "Meet the fleet";
export const LOOP_ANCHOR_ID = "operational-loop";

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

export const AGENTS_SECTION_HEADING = "A fleet, ready to run.";

export const AGENTS_SECTION_LEDE =
  "Prebuilt and proven. Install one, wire a single source, and it works the same day — every action gated, every run a replayable log. More join the fleet as the loop compounds.";

export type AgentIntegration = {
  label: string;
  icon: string;
};

export type PrebuiltAgent = {
  id: string;
  category: string;
  name: string;
  description: string;
  integrations: readonly AgentIntegration[];
  // Roadmap/forward-looking agents (not yet a shipped prebuilt) render a
  // "coming soon" badge and a waitlist CTA instead of "Try it".
  comingSoon?: boolean;
};

const INTEGRATION_ICONS = {
  github: { label: "GitHub", icon: "/logos/github.svg" },
  fly: { label: "Fly.io", icon: "/logos/fly.svg" },
  grafana: { label: "Grafana", icon: "/logos/grafana.svg" },
  slack: { label: "Slack", icon: "/logos/slack.svg" },
} as const satisfies Record<string, AgentIntegration>;

export const PREBUILT_AGENTS: readonly PrebuiltAgent[] = [
  {
    id: "auto-reviewer",
    category: "Code review",
    name: "Auto Reviewer",
    description:
      "Wakes on every pull request, runs a full review against the diff and the surrounding code, and pushes inline feedback before a human opens the tab.",
    integrations: [INTEGRATION_ICONS.github],
  },
  {
    id: "diagnose",
    category: "Incident response",
    name: "Diagnose the Problem",
    description:
      "Triggered from GitHub, it pulls logs from Fly and dashboards from Grafana, correlates the failure into one cause, and posts the diagnosis to Slack.",
    integrations: [
      INTEGRATION_ICONS.github,
      INTEGRATION_ICONS.fly,
      INTEGRATION_ICONS.grafana,
      INTEGRATION_ICONS.slack,
    ],
  },
  {
    id: "security-reviewer",
    category: "Security",
    name: "Security Reviewer",
    description:
      "Scans each pull request and its dependencies for vulnerabilities and exposed secrets, reproduces the finding, opens a remediation pull request, and holds the fix at human approval while flagging the team in Slack.",
    integrations: [INTEGRATION_ICONS.github, INTEGRATION_ICONS.slack],
    comingSoon: true,
  },
] as const;

export type AgentPillar = {
  id: string;
  eyebrow: string;
  title: string;
  description: string;
};

export const AGENT_PILLARS: readonly AgentPillar[] = [
  {
    id: "sandbox",
    eyebrow: "Isolated",
    title: "A private machine per run",
    description:
      "Every agent runs in its own clean sandbox — a dedicated shell, tools, and scratch space. It clones the workspace, reads the context, writes the patch, and runs the checks without trampling anyone else's environment.",
  },
  {
    id: "learns",
    eyebrow: "Compounding",
    title: "It learns how you operate",
    description:
      "No constant supervision. It sits inside the systems you connect, watches the work that actually happens, and models your stack in days, not weeks. Every recurrence starts where the last one left off.",
  },
  {
    id: "proactive",
    eyebrow: "Proactive",
    title: "It moves first",
    description:
      "Wake it on an event, a schedule, or a webhook and it takes the initiative — watching deploys, logs, and tickets, surfacing only what needs you. And every step it takes is a replayable log you can audit line by line.",
  },
] as const;

export const OPERATIONAL_KNOWLEDGE_HEADING =
  "Recurring tickets become operational memory.";

export const OPERATIONAL_KNOWLEDGE_LEDE =
  "The first signal is not the product. The compounding layer is: classify the problem, preserve the evidence, generate a scenario and a test, then turn the approved fix into memory for the next recurrence.";

export const KNOWLEDGE_POINTS = [
  {
    number: "01",
    title: "Classify the problem",
    description:
      "The run turns scattered support symptoms into a named engineering problem class.",
  },
  {
    number: "02",
    title: "Prove it with evidence",
    description:
      "Logs, traces, repository context, and approval state stay replayable instead of disappearing into chat.",
  },
  {
    number: "03",
    title: "Ship only through people",
    description:
      "The agent can draft a fix pull request, but merge and deploy stay behind human approval.",
  },
] as const;

export const HOW_IT_WORKS_HEADING =
  "From first signal to fewer repeats.";

export const LOOP_STEPS = [
  {
    number: "01",
    title: "A signal arrives",
    description:
      "A support escalation, workflow event, cron, or manual steer lands on the event stream with actor provenance.",
  },
  {
    number: "02",
    title: "The agent gathers evidence",
    description:
      "It reads only the allow-listed sources: telemetry, repository context, approvals, and prior run history.",
  },
  {
    number: "03",
    title: "The problem class is named",
    description:
      "The run stops treating the ticket as a one-off and records the recurring failure class.",
  },
  {
    number: "04",
    title: "A scenario is generated",
    description:
      "The agent writes the situation back as a reproducible scenario the team can inspect.",
  },
  {
    number: "05",
    title: "A regression test follows",
    description:
      "The class gets a test so the same failure has a durable tripwire next time.",
  },
  {
    number: "06",
    title: "A fix pull request opens",
    description:
      "Code changes arrive with the evidence trail attached, not as an ungrounded suggestion.",
  },
  {
    number: "07",
    title: "Humans approve",
    description:
      "Risky actions hold at the approval plane. People merge and deploy.",
  },
  {
    number: "08",
    title: "The class is remembered",
    description:
      "The next recurrence starts with the captured problem class, scenario, test, and fix trail.",
  },
] as const;

export const CAPABILITY_HEADING = "The trust layer, not a wrapper.";

export const CAPABILITY_ITEMS = [
  {
    number: "01",
    title: "Sandboxed runtime",
    description:
      "Bounded execution by construction. Every tool call runs inside the configured blast radius.",
  },
  {
    number: "02",
    title: "Vaulted credentials",
    description:
      "Secrets resolve at the tool boundary from the vault. They are not printed into prompts, logs, or tables.",
  },
  {
    number: "03",
    title: "Approval gating",
    description:
      "Risky work blocks until a human approves. State survives worker restarts.",
  },
  {
    number: "04",
    title: "Open source + replay",
    description:
      "The runtime is code you can read, and every run stays replayable from the event log.",
  },
] as const;

export const PRICING_COPY = {
  trialSuffix: "events and runtime on us",
  headline: "Start free. Pay only while it runs.",
  lede:
    "Usage is metered per second — no seats, no tiers tax. Enterprise adds the controls large teams need.",
  note:
    "Usage rates are fixed and cross-tier-pinned. Enterprise is contact-only; no fabricated tier price.",
  enterpriseCta: "Talk to us",
} as const;

export const PRICING_PLANS = [
  {
    id: "trial",
    name: "Free trial",
    price: "$0",
    suffix: "until Jul 31",
    features: ["Every event free", "Every run free", "Starter credit included", "Full product access"],
    cta: "Start free",
    featured: false,
  },
  {
    id: "usage",
    name: "Usage",
    badge: "Team",
    price: "metered",
    suffix: "per second",
    features: ["Starter credit included", "Events always free", "Metered only while running", "Pay as you go"],
    cta: HERO_PRIMARY_LABEL,
    featured: true,
  },
  {
    id: "enterprise",
    name: "Enterprise",
    price: "Custom",
    features: ["Single Sign-On (SSO)", "Audit export", "Dedicated runners", "Priority support"],
    cta: PRICING_COPY.enterpriseCta,
    featured: false,
  },
] as const;

export const FAQ_WEDGE_ITEM = {
  q: "What does the agent read?",
  a: "Signals, telemetry, code, and control-plane state — only the sources you allow-list. It uses them to classify the problem, produce evidence, and stop at human approval before merge or deploy.",
} as const;

export const CTA_COPY = {
  heading: "Give your hardest incidents an engineer.",
  lede:
    "Install one agent, wire the first source, and let recurring support escalations become scenario-backed fixes with a human gate.",
} as const;

export const FORBIDDEN_MARKETING_CLAIMS = [
  "zero tickets",
  "autonomous merge",
  "autonomous deploy",
  "40%",
  "hour response time",
  "ticket latency",
] as const;

export type KnowledgePoint = (typeof KNOWLEDGE_POINTS)[number];
export type LoopStep = (typeof LOOP_STEPS)[number];
export type CapabilityItem = (typeof CAPABILITY_ITEMS)[number];
export type PricingPlan = (typeof PRICING_PLANS)[number];
