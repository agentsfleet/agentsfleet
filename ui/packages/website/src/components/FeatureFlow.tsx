import { Card } from "@usezombie/design-system";
import { APP_BASE_URL, DOCS_QUICKSTART_URL, DOCS_URL } from "../config";

type FeatureFlowItem = {
  id: string;
  title: string;
  description: string;
  bullets: string[];
  ctaLabel: string;
  ctaHref: string;
  panel: ReadonlyArray<string>;
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
    ctaLabel: "install guide",
    ctaHref: DOCS_QUICKSTART_URL,
    panel: ["$ npm install -g @usezombie/zombiectl"],
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
    ctaLabel: "read docs",
    ctaHref: DOCS_URL,
    panel: [
      "zombie_id: zmb_2041",
      "event:     workflow_run.failed",
      "status:    notified",
      "slack:     #platform-ops",
    ],
  },
  {
    id: "mission",
    title: "Mission Control",
    description:
      "Approvals, budgets, BYOK provider, kill switch — one dashboard. Approve from Slack or the web.",
    bullets: [
      "Daily + monthly dollar caps, trip-blocked at the gate",
      "zombiectl kill checkpoints state; nothing lost",
    ],
    ctaLabel: "open mission control",
    ctaHref: APP_BASE_URL,
    panel: [
      "agents     12",
      "approvals   3",
      "credits  $7.40",
    ],
  },
];

/*
 * FeatureFlow — 3 alternating evidence rows. Each row pairs a mono code
 * panel with a sans copy column. No orange rail decoration; borders carry
 * the elevation. Reverses on odd rows for visual rhythm only.
 */
export default function FeatureFlow() {
  return (
    <section className="site-section" aria-label="Feature flow" data-testid="feature-flow">
      <div className="wrap flex flex-col gap-16">
        {items.map((item, index) => {
          const reversed = index % 2 === 1;
          return (
            <article
              key={item.id}
              className={`grid gap-8 items-center grid-cols-1 lg:grid-cols-2 ${reversed ? "lg:[&>*:first-child]:order-2" : ""}`}
              data-testid={`feature-flow-${item.id}`}
            >
              <Card className="font-mono text-[13px] leading-[1.7] text-text-muted whitespace-pre-line">
                {item.panel.join("\n")}
              </Card>
              <div className="flex flex-col gap-4">
                <h3 className="font-mono text-[clamp(20px,2.5vw,28px)] leading-[1.2] tracking-[-0.015em] text-text font-medium m-0">
                  {item.title}
                </h3>
                <p className="font-sans text-[15px] leading-[1.6] text-text-muted m-0">
                  {item.description}
                </p>
                <ul className="list-none p-0 m-0 flex flex-col gap-2">
                  {item.bullets.map((bullet) => (
                    <li
                      key={bullet}
                      className="font-mono text-[13px] text-text-muted before:content-['↳'] before:mr-2 before:text-text-subtle"
                    >
                      {bullet}
                    </li>
                  ))}
                </ul>
                <a
                  href={item.ctaHref}
                  className="font-mono text-[13px] text-pulse hover:underline"
                >
                  {item.ctaLabel} →
                </a>
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}
