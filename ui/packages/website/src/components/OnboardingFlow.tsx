import { Card, Terminal } from "@agentsfleet/design-system";
import { DOCS_QUICKSTART_URL, INSTALL_COMMAND, INSTALL_SKILL_SLASH } from "../config";

type Step = {
  id: string;
  number: string;
  title: string;
  command: string;
  caption: string;
};

const STEPS: readonly Step[] = [
  {
    id: "install",
    number: "01",
    title: "Install the CLI",
    command: INSTALL_COMMAND,
    caption: "One command — installs agentsfleet + the skill bundle, host-detected (Claude Code, Amp, Codex, OpenCode).",
  },
  {
    id: "skill",
    number: "02",
    title: "Run the install skill",
    command: INSTALL_SKILL_SLASH,
    caption:
      "Drop the slash command into Claude Code, or paste TRIGGER.md + SKILL.md at /zombies/new in the Dashboard.",
  },
  {
    id: "wire",
    number: "03",
    title: "Wire your trigger",
    command:
      'gh api -X POST repos/<OWNER/REPO>/hooks \\\n  -F "events[]=workflow_run" \\\n  -F "config[url]=<WEBHOOK_URL>" \\\n  -F "config[secret]=<SECRET>"',
    caption:
      "Or copy the rendered command from the Dashboard's per-provider GuidedTriggerCard — variables substituted in.",
  },
  {
    id: "steer",
    number: "04",
    title: "Steer your agent",
    command: 'agentsfleet steer <zombie_id> "morning health check"',
    caption:
      "Or type into the Dashboard chat composer on /zombies/{id}. Every wake lands on the durable event log.",
  },
];

export default function OnboardingFlow() {
  return (
    <section
      id="onboarding-flow"
      className="site-section"
      aria-label="Onboarding flow"
      data-testid="onboarding-flow"
    >
      <div className="wrap flex flex-col gap-8">
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          {STEPS.map((step) => (
            <FlowStep key={step.id} step={step} />
          ))}
        </div>
        <a
          href={DOCS_QUICKSTART_URL}
          className="font-mono text-mono text-text-muted hover:text-pulse hover:underline self-start"
          data-testid="onboarding-flow-quickstart"
        >
          read the full quickstart →
        </a>
      </div>
    </section>
  );
}

function FlowStep({ step }: { step: Step }) {
  return (
    <Card
      className="flex min-w-0 flex-col gap-3 p-6"
      data-testid={`onboarding-step-${step.id}`}
    >
      <span
        className="font-mono text-eyebrow uppercase tracking-eyebrow text-pulse"
        data-testid={`onboarding-step-number-${step.id}`}
      >
        step {step.number}
      </span>
      <h3 className="font-mono text-fluid-display-md leading-display-md tracking-display-md text-text font-medium m-0">
        {step.title}
      </h3>
      <Terminal
        label={`${step.title} command`}
        copyable
        className="flex-1 min-w-0"
        data-testid={`onboarding-step-command-${step.id}`}
      >
        {step.command}
      </Terminal>
      <p className="font-sans text-body-sm leading-body text-text-muted m-0">
        {step.caption}
      </p>
    </Card>
  );
}
