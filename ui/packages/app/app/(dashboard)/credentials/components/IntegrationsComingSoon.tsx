import { Badge } from "@agentsfleet/design-system";
import { BriefcaseIcon, GitPullRequestIcon, HashIcon } from "lucide-react";

// Integrations render as "Planned" with no Connect control — the first-class
// one-click connectors (Connect → auth flow → minted token) are a separate
// milestone. Until they land, a fleet reaches these tools by storing the token
// as a custom secret (the bridge hint below). Config-driven rows (RULE CFG):
// adding a future integration is a data row, not a new branch.

const PLANNED_LABEL = "Planned";

type Integration = {
  name: string;
  description: string;
  Icon: React.ComponentType<{ size?: number }>;
};

const INTEGRATIONS: readonly Integration[] = [
  {
    name: "GitHub",
    description: "Run fleets on issues, pull requests, and CI failures.",
    Icon: GitPullRequestIcon,
  },
  {
    name: "Zoho",
    description: "Summarize Sprints, act on Desk tickets.",
    Icon: BriefcaseIcon,
  },
  {
    name: "Slack",
    description: "Mention a fleet in channels; post run results.",
    Icon: HashIcon,
  },
] as const;

function IntegrationRow({ name, description, Icon }: Integration) {
  return (
    <div
      data-testid={`integration-${name.toLowerCase()}`}
      className="flex items-center gap-3 rounded-md border border-border bg-card px-4 py-3"
    >
      <span className="flex-none text-muted-foreground" aria-hidden="true">
        <Icon size={16} />
      </span>
      <div className="min-w-0 flex-1">
        <div className="font-medium text-foreground">{name}</div>
        <div className="text-xs text-muted-foreground">{description}</div>
      </div>
      <Badge variant="default">{PLANNED_LABEL}</Badge>
    </div>
  );
}

export default function IntegrationsComingSoon() {
  return (
    <div className="space-y-3" data-testid="integrations-coming-soon">
      <p className="text-xs text-muted-foreground">
        Connectors are on the roadmap — until then, store an integration token as a{" "}
        <span className="font-medium text-foreground">custom secret</span> above (e.g.{" "}
        <code className="font-mono">GITHUB_TOKEN</code>) and your fleets use it today.
      </p>
      <div className="flex flex-col gap-2">
        {INTEGRATIONS.map((integration) => (
          <IntegrationRow key={integration.name} {...integration} />
        ))}
      </div>
    </div>
  );
}
