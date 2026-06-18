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
export const HERO_SECONDARY_LABEL = "See the loop";
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

export type LedgerLine = {
  id: string;
  timestamp: string;
  tag?: string;
  tagSeverity?: "info" | "evidence" | "pulse";
  severity?: "info" | "debug" | "done";
  message: string;
};

export const LEDGER_LINES: readonly LedgerLine[] = [
  {
    id: "install",
    timestamp: "00:00.0",
    tag: "[setup]",
    tagSeverity: "pulse",
    severity: "info",
    message: "install agent, register skill, wire the first source",
  },
  {
    id: "wake",
    timestamp: "00:00.2",
    tag: "[wake]",
    tagSeverity: "info",
    severity: "info",
    message: "first signal received — support escalation crossed into engineering work",
  },
  {
    id: "investigate",
    timestamp: "00:00.6",
    tag: "[work]",
    tagSeverity: "info",
    severity: "debug",
    message: "pulling traces, logs, recent deploys, and repository context",
  },
  {
    id: "problem-class",
    timestamp: "00:01.4",
    tag: "[EVIDENCE]",
    tagSeverity: "evidence",
    severity: "info",
    message: "recurring problem class: connection-pool exhaustion under retry storm",
  },
  {
    id: "scenario-test",
    timestamp: "00:02.1",
    tag: "[work]",
    tagSeverity: "info",
    severity: "debug",
    message: "generated scenario and regression test for the class",
  },
  {
    id: "fix-pr",
    timestamp: "00:02.8",
    tag: "[work]",
    tagSeverity: "info",
    severity: "debug",
    message: "opened fix pull request — bound pool, jittered backoff",
  },
  {
    id: "approval",
    timestamp: "00:02.8",
    tag: "[gate]",
    tagSeverity: "pulse",
    severity: "info",
    message: "awaiting human approval — merge and deploy held",
  },
  {
    id: "learn",
    timestamp: "00:14.9",
    severity: "done",
    message: "approved and merged by a human · problem class captured · recurrence reduced",
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
    badge: "live model",
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
