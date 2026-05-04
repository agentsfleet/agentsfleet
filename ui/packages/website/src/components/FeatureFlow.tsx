import { Card } from "@usezombie/design-system";
import { APP_BASE_URL, DOCS_QUICKSTART_URL, DOCS_URL } from "../config";

type FeatureFlowItem = {
  id: string;
  title: string;
  description: string;
  bullets: string[];
  ctaLabel: string;
  ctaHref: string;
  panel: "install" | "trace" | "mission";
};

const items: FeatureFlowItem[] = [
  {
    id: "install",
    title: "Install once.",
    description:
      "One command installs the CLI; one skill installs the platform-ops agent. Host-neutral — Claude Code, Amp, Codex CLI, or OpenCode.",
    bullets: [
      "Detects fly.toml, GitHub Actions, monorepos",
      "Idempotent re-runs",
    ],
    ctaLabel: "Install guide",
    ctaHref: DOCS_QUICKSTART_URL,
    panel: "install",
  },
  {
    id: "trace",
    title: "Every event on the record.",
    description:
      "Steer, webhook, cron — all land on the same event stream with actor provenance. Replay the timeline; stream live via SSE.",
    bullets: [
      "actor = webhook | cron | steer | continuation",
      "Stage chunking carries reasoning past the model's context cap",
    ],
    ctaLabel: "Read docs",
    ctaHref: DOCS_URL,
    panel: "trace",
  },
  {
    id: "mission",
    title: "Mission Control",
    description:
      "Approvals, budgets, BYOK provider, kill switch — one dashboard. Approve from Slack or the web.",
    bullets: [
      "Daily + monthly dollar caps, trip-blocked at the gate",
      "`zombiectl kill` checkpoints state; nothing lost",
    ],
    ctaLabel: "Open Mission Control",
    ctaHref: APP_BASE_URL,
    panel: "mission",
  },
];

function Panel({ kind }: { kind: FeatureFlowItem["panel"] }) {
  if (kind === "install") {
    return (
      <div className="feature-flow-panel-shell">
        <p className="feature-flow-code">$ npm install -g @usezombie/zombiectl</p>
      </div>
    );
  }

  if (kind === "trace") {
    return (
      <div className="feature-flow-panel-shell">
        <p className="feature-flow-code">
          zombie_id: zmb_2041
          <br />
          event: workflow_run.failed
          <br />
          status: notified
          <br />
          slack: #platform-ops
        </p>
      </div>
    );
  }

  return (
    <div className="feature-flow-panel-shell feature-flow-mission-grid" aria-hidden="true">
      <span>Agents</span>
      <span>12</span>
      <span>Approvals</span>
      <span>3</span>
      <span>Credits</span>
      <span>$7.40</span>
    </div>
  );
}

export default function FeatureFlow() {
  return (
    <section className="section-gap feature-flow-wrap" aria-label="Feature flow">
      {items.map((item, index) => (
        <Card
          key={item.id}
          asChild
          className="!bg-transparent !border-0 !rounded-none !p-0 hover:!shadow-none"
        >
        <article className={`feature-flow-row ${index % 2 === 1 ? "reverse" : ""}`}>
          <Panel kind={item.panel} />
          <div className="feature-flow-copy">
            <h3>{item.title}</h3>
            <p>{item.description}</p>
            <ul>
              {item.bullets.map((bullet) => (
                <li key={bullet}>{bullet}</li>
              ))}
            </ul>
            <a className="cta ghost feature-flow-cta" href={item.ctaHref}>
              {item.ctaLabel} &rarr;
            </a>
          </div>
        </article>
        </Card>
      ))}
    </section>
  );
}
