export const PRODUCT_NAME = "agentsfleet";

export const HERO_HEADLINE = "AI teammates for incident response.";

// Tokens that must survive in the hero copy (marketing-spec.test.ts pins
// presence). They double as the "Pillars" bullets in llms-full.txt, so keep
// them phrase-shaped and meaningful, not single words.
export const PILLAR_TOKENS = [
  "AI incident teammate",
  "logs, metrics, and code",
  "You control access and decide what ships.",
] as const;

// Lead with the work a visitor can delegate, then explain the control boundary.
export const HERO_LEDE_PARTS = {
  intro: "Your",
  teammates: "AI incident teammate",
  middle: "investigates failures using your",
  recurringWork: "logs, metrics, and code",
  outro: "to explain what went wrong and help prepare a fix. You control access and decide what ships.",
} as const;

export const HERO_PRIMARY_LABEL = "Request early access";
export const HERO_SECONDARY_LABEL = "See how it works";
export const HOW_IT_WORKS_ANCHOR_ID = "how-it-works";
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

// The catalogue anchor matches the hero's secondary action.
export const FLEETS_SECTION_HEADING = "Meet the fleet.";

export const FLEETS_SECTION_LEDE =
  "Start with a job you want off your plate. Configure a prebuilt fleet for your stack, or connect a channel teammate in Slack. Explore the workflows below; hosted access is through the waitlist.";

export type FleetIntegration = {
  label: string;
  icon: string;
};

export type PrebuiltFleet = {
  id: string;
  category: string;
  name: string;
  description: string;
  trigger: string;
  output: string;
  control: string;
  integrations: readonly FleetIntegration[];
  // Availability is separate from the hosted-access waitlist.
  comingSoon?: boolean;
};

export const INTEGRATION_ICONS = {
  github: { label: "GitHub", icon: "/logos/github.svg" },
  grafana: { label: "Grafana", icon: "/logos/grafana.svg" },
  elasticsearch: { label: "Elasticsearch", icon: "/logos/elasticsearch.svg" },
  slack: { label: "Slack", icon: "/logos/slack.svg" },
} as const satisfies Record<string, FleetIntegration>;

export const PREBUILT_FLEETS: readonly PrebuiltFleet[] = [
  {
    id: "diagnose",
    category: "Incident response",
    name: "Incident Response",
    description:
      "Find the cause of an incident, then prepare a fix for your team to review.",
    trigger: "Scheduled telemetry checks, a failed GitHub workflow, or a request from your team.",
    output: "A diagnosis using Grafana and Elasticsearch, plus a draft pull request when a fix is appropriate.",
    control: "Approve repository write access, then review and merge the diff. The fleet never merges or deploys.",
    integrations: [
      INTEGRATION_ICONS.github,
      INTEGRATION_ICONS.grafana,
      INTEGRATION_ICONS.elasticsearch,
      INTEGRATION_ICONS.slack,
    ],
  },
  {
    id: "slack-teammate",
    category: "Channel teammate",
    name: "Slack Teammate",
    description: "Connect Slack for a teammate that carries channel memory across threads.",
    trigger: "An @agentsfleet mention in a channel where the bot is invited.",
    output: "An in-thread answer informed by that channel’s saved context.",
    control: "Mention-only and read-only. It never acts unattended or changes your systems.",
    integrations: [INTEGRATION_ICONS.slack],
  },
  {
    id: "auto-reviewer",
    category: "Code review",
    name: "PR Reviewer",
    description:
      "Give your team another set of eyes on the diff and its surrounding code.",
    trigger: "Selected pull-request events in configured repositories, after you connect GitHub and approve access.",
    output: "Review comments on the pull request.",
    control: "You choose the repositories and permissions. You review and merge.",
    integrations: [INTEGRATION_ICONS.github],
  },
  {
    id: "security-reviewer",
    category: "Security",
    name: "Security Reviewer",
    description:
      "Planned: investigate vulnerabilities and exposed secrets in your own code and dependencies.",
    trigger: "Planned pull-request and scheduled scans.",
    output: "Proposed findings with evidence and a remediation pull request.",
    control: "Not available yet. Planned remediation stays subject to human approval.",
    integrations: [INTEGRATION_ICONS.github, INTEGRATION_ICONS.slack],
    comingSoon: true,
  },
] as const;

export type FleetPillar = {
  id: string;
  eyebrow: string;
  title: string;
  description: string;
};

// The three behavioral capabilities. Moved out of the prebuilt-fleets wall and
// into Core Capabilities (design-consultation decision) — they describe what
// every teammate is, not which prebuilts exist.
export const FLEET_PILLARS: readonly FleetPillar[] = [
  {
    id: "sandbox",
    eyebrow: "Isolated",
    title: "Access you control",
    description:
      "Fleets work within their configured runtime and tool permissions. Give each job the access it needs, and keep repository writes behind approval.",
  },
  {
    id: "learns",
    eyebrow: "Compounding",
    title: "Context for the next job",
    description:
      "Saved fleet memory carries useful context into later runs. The Slack teammate keeps channel context across threads, without reading other channels’ memory.",
  },
  {
    id: "proactive",
    eyebrow: "Proactive",
    title: "Starts when you choose",
    description:
      "Configure supported events, schedules, or a manual request. Inspect the run history to understand what happened. The Slack channel teammate stays mention-only.",
  },
] as const;

export const HOW_IT_WORKS_HEADING = "From scattered clues to a clear next step.";

// The incident example combines separately configured fleets; other triggers
// and destinations use the same bounded execution model.
export const HOW_IT_WORKS_FOOTNOTE =
  "This example combines separately configured incident fleets. Connect the required evidence sources and Slack destination; approve repository access before repair runs.";

// Machine-readable loop copy for llms-full.txt. The visual website example is
// owned by FleetPreview, while this sequence explains a concrete PR-review run.
export const LOOP_STEPS = [
  {
    number: "01",
    title: "You push a pull request",
    description:
      "Connect GitHub, select your repositories and events, and approve the grants. Matching pull-request events then wake the PR Reviewer.",
  },
  {
    number: "02",
    title: "It posts the review",
    description:
      "It reads the diff and the code around it, then leaves review comments for your team.",
  },
  {
    number: "03",
    title: "Slack gets the heads-up",
    description:
      "Your team sees the verdict in Slack and decides — approve, merge, or steer. You stay the one who ships.",
  },
] as const;

export const CAPABILITY_HEADING = "Helpful teammates. You stay in control.";

export const RUNTIME_GUARANTEES_LABEL =
  "Controls for every job";

export const CAPABILITY_ITEMS = [
  {
    number: "01",
    title: "Isolated workspaces",
    description:
      "Each fleet works within the environment and permissions you configure.",
  },
  {
    number: "02",
    title: "Protected credentials",
    description:
      "Your credentials stay in the vault. Tools use them when needed without including them in prompts or logs.",
  },
  {
    number: "03",
    title: "Your approval matters",
    description:
      "Work that requires approval waits for you. Pending approvals survive worker restarts.",
  },
  {
    number: "04",
    title: "Run history & budgets",
    description:
      "Inspect run history and spending. Configure fleet budget limits; an active run can cross a limit before its next budget check.",
  },
] as const;

export const PRICING_COPY = {
  headline: "Bring a real workflow. Help shape what’s next.",
  lede: "We’re looking for founders and infrastructure teams to try agentsfleet on their own work and tell us what helps, what breaks, and what’s missing.",
  status: "Early access · pricing is being worked out",
  note: "Pricing will be confirmed before paid usage. Early-access terms and any usage limits will be shared before you start.",
  runtime: "Fleet runtime covers the work agentsfleet runs for you.",
  models: "Model usage is separate. With your own model key, you pay your provider directly; that does not remove fleet runtime costs.",
} as const;

export const FAQ_WEDGE_ITEM = {
  q: "What does the Fleet read?",
  a: "Signals, telemetry, code, and control-plane state — only the sources you allow-list. It uses them to classify the problem, produce evidence, and stop at human approval before merge or deploy.",
} as const;

export const CTA_COPY = {
  heading: "Start with one job you’d like to delegate.",
  lede:
    "Pick a repository or a recurring investigation. Read the setup guide, choose the access your fleet needs, and review its first result.",
} as const;

export const FORBIDDEN_MARKETING_CLAIMS = [
  "zero tickets",
  "autonomous merge",
  "autonomous deploy",
  "40%",
  "hour response time",
  "ticket latency",
] as const;

export type LoopStep = (typeof LOOP_STEPS)[number];
export type CapabilityItem = (typeof CAPABILITY_ITEMS)[number];
