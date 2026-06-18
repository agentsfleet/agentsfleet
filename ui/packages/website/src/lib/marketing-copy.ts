export const PRODUCT_NAME = "agentsfleet";

export const HERO_HEADLINE = "A fleet, ready to run.";

// Tokens that must survive in the hero copy (marketing-spec.test.ts pins
// presence). They double as the "Pillars" bullets in llms-full.txt, so keep
// them phrase-shaped and meaningful, not single words.
export const PILLAR_TOKENS = [
  "AI teammates",
  "ready to run",
  "recurring engineering work",
  "wake.on.event",
] as const;

// Hero lede, warm/anti-jargon voice (design-consultation decision). Rendered as
// one sentence with two emphasized phrases. Verbatim payoff line the user
// approved in the hero preview: "Prebuilt AI teammates that take the recurring
// engineering work off your plate — and hand you the change to approve."
export const HERO_LEDE_PARTS = {
  intro: "Prebuilt",
  teammates: "AI teammates",
  middle: "that take the",
  recurringWork: "recurring engineering work",
  outro: "off your plate — and hand you the change to approve.",
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

// The hero owns "A fleet, ready to run." now, so the prebuilt-agents wall gets
// its own heading. "Meet the fleet" matches the hero's secondary CTA, which
// anchors to this section.
export const AGENTS_SECTION_HEADING = "Meet the fleet.";

export const AGENTS_SECTION_LEDE =
  "Prebuilt and proven. Install one, point it at your stack, and it works the same day — every action gated, every run a replayable log. More teammates join the fleet over time.";

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

// The three behavioral capabilities. Moved out of the prebuilt-agents wall and
// into Core Capabilities (design-consultation decision) — they describe what
// every teammate is, not which prebuilts exist.
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
  "It remembers, so the next time is faster.";

export const OPERATIONAL_KNOWLEDGE_LEDE =
  "Every fix becomes memory. When the same problem comes back, your teammate already knows its shape — the scenario it wrote, the test it added, and the change that worked last time.";

export const KNOWLEDGE_POINTS = [
  {
    number: "01",
    title: "It names the problem",
    description:
      "The first ticket stops being a one-off. Your teammate files it as a named, recurring problem instead of scattered symptoms.",
  },
  {
    number: "02",
    title: "It keeps the receipts",
    description:
      "Logs, traces, and the change that fixed it stay together and replayable, instead of vanishing into a chat thread.",
  },
  {
    number: "03",
    title: "You still hold the merge",
    description:
      "It drafts the fix, but merging and shipping always wait for a human.",
  },
] as const;

export const HOW_IT_WORKS_HEADING = "Push a pull request. Get a review back.";

// Three opinionated beats, rendered as a left-to-right flow in HowItWorks.tsx.
// The concrete Auto Reviewer path (PR -> review -> Slack) stands in for the
// loop; it reads far better than the old eight-step abstraction.
export const LOOP_STEPS = [
  {
    number: "01",
    title: "You push a pull request",
    description:
      "The Auto Reviewer wakes the moment the PR lands. No prompt, no queue, no setup.",
  },
  {
    number: "02",
    title: "It posts the review",
    description:
      "It reads the diff and the code around it, then leaves inline comments before a human opens the tab.",
  },
  {
    number: "03",
    title: "Slack gets the heads-up",
    description:
      "Your team sees the verdict in Slack and decides — approve, merge, or steer. You stay the one who ships.",
  },
] as const;

export const CAPABILITY_HEADING = "What every teammate ships with.";

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
  heading: "Meet the teammates who never skip the boring work.",
  lede:
    "Chat with your fleet like colleagues. They watch the work that keeps recurring, take the first pass, and hand you a change to approve. No prompting, no setup, no jargon.",
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
