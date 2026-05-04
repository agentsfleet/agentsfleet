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
    title: "Install once. Operate forever.",
    description:
      "One command installs zombiectl, one skill installs the platform-ops agent. The skill detects your repo shape, asks three gating questions, and writes .usezombie/platform-ops/SKILL.md + TRIGGER.md.",
    bullets: [
      "Host-neutral skill: Claude Code, Amp, Codex CLI, OpenCode",
      "Detects fly.toml, GitHub Actions workflows, monorepo layouts",
      "Idempotent re-runs against the same workspace",
    ],
    ctaLabel: "Install guide",
    ctaHref: DOCS_QUICKSTART_URL,
    panel: "install",
  },
  {
    id: "trace",
    title: "Every event, every actor, on the record.",
    description:
      "Every steer, webhook, and cron fire lands on zombie:{id}:events with actor provenance. Replay the full timeline. Stream live via SSE. Audit who or what triggered each step.",
    bullets: [
      "Append-only event stream with actor=webhook|cron|steer|continuation",
      "SSE tail at /v1/.../events/stream",
      "Stage chunking preserves long-running reasoning across context boundaries",
    ],
    ctaLabel: "Read docs",
    ctaHref: DOCS_URL,
    panel: "trace",
  },
  {
    id: "mission",
    title: "Mission Control",
    description:
      "Approvals, budgets, BYOK provider switching, and the kill switch — one dashboard. Approve a risky action from a Slack DM or the web.",
    bullets: [
      "Per-day and per-month dollar caps; trip-blocked at the gate",
      "Switch BYOK provider with `zombiectl tenant provider set --credential <name>`",
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
