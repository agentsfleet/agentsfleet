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
    title: "Install once. Start shipping PRs.",
    description:
      "Install zombiectl, connect GitHub, and keep your team workflow intact while UseZombie handles execution and PR delivery.",
    bullets: [
      "Works with existing coding agents and IDE-first teams",
      "No workflow migration required",
      "Automated PR lifecycle with harness checks before review",
    ],
    ctaLabel: "Install guide",
    ctaHref: DOCS_QUICKSTART_URL,
    panel: "install",
  },
  {
    id: "trace",
    title: "Traceability and replay by default",
    description:
      "Track each run from intent to merged PR with event history, validation output, replay, and M9 quality signals that show whether agents are actually improving.",
    bullets: [
      "Replay failed runs with deterministic artifacts and clearer context",
      "See why a run degraded before it becomes review churn",
      "Use score history and failure analysis to guide improvements",
    ],
    ctaLabel: "Read docs",
    ctaHref: DOCS_URL,
    panel: "trace",
  },
  {
    id: "mission",
    title: "Mission Control",
    description:
      "Centralize run visibility, profile behavior, and rollout guardrails so teams can scale automation without giving up control.",
    bullets: [
      "Observe quality trends and policy outcomes in one place",
      "Tune agent profiles by repo and team with auditability",
      "Paid tiers add stronger sandbox governance for untrusted code paths",
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
        <p className="feature-flow-code">$ curl -fsSL https://usezombie.sh/install.sh | bash</p>
      </div>
    );
  }

  if (kind === "trace") {
    return (
      <div className="feature-flow-panel-shell">
        <p className="feature-flow-code">
          run_id: zmb_2041
          <br />
          stage: validate
          <br />
          status: passed
          <br />
          pr: github.com/org/repo/pull/482
        </p>
      </div>
    );
  }

  return (
    <div className="feature-flow-panel-shell feature-flow-mission-grid" aria-hidden="true">
      <span>AI Code</span>
      <span>78%</span>
      <span>Runs</span>
      <span>243</span>
      <span>Merged</span>
      <span>91%</span>
    </div>
  );
}

export default function FeatureFlow() {
  return (
    <section className="section-gap feature-flow-wrap" aria-label="Feature flow">
      {items.map((item, index) => (
        <article
          key={item.id}
          className={`feature-flow-row ${index % 2 === 1 ? "reverse" : ""}`}
        >
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
      ))}
    </section>
  );
}
