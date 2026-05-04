import { Link } from "react-router-dom";
import Hero from "../components/Hero";
import FeatureSection from "../components/FeatureSection";
import FeatureFlow from "../components/FeatureFlow";
import HowItWorks from "../components/HowItWorks";
import { InstallBlock } from "@usezombie/design-system";
import { DOCS_QUICKSTART_URL, DOCS_URL } from "../config";

const features = [
  {
    number: "01",
    title: "Markdown-defined behaviour",
    description:
      "SKILL.md + TRIGGER.md. Iterate on prose, not redeploys.",
  },
  {
    number: "02",
    title: "Three triggers, one loop",
    description:
      "Webhook (GitHub Actions), cron, and `zombiectl steer` all flow through the same reasoning. The agent doesn't branch on actor type.",
  },
  {
    number: "03",
    title: "Bring Your Own Key",
    description:
      "Anthropic, OpenAI, Fireworks (Kimi K2), Together, Groq, Moonshot. The executor treats your provider key as another secret resolved at the tool bridge.",
  },
  {
    number: "04",
    title: "Reasons past the context limit",
    description:
      "Memory checkpoints, rolling tool-result window, and stage chunking compose so deep incidents continue past the model's working-memory cap.",
  },
  {
    number: "05",
    title: "Approval gating",
    description:
      "Risky actions block until a human clicks Approve in the dashboard or Slack. State machine survives worker restarts.",
  },
  {
    number: "06",
    title: "Open-source runtime",
    description:
      "The code that holds your credentials and runs against your infrastructure is code you can read.",
  },
];

export default function Home() {
  return (
    <section className="stack home-stack route-fade">
      <Hero />
      <FeatureFlow />
      <div className="section-gap home-section-head">
        <p className="eyebrow">Core capabilities</p>
        <h2>A long-lived runtime that owns the outcome until it&apos;s resolved or blocked.</h2>
      </div>
      <div className="grid two features-grid">
        {features.map((f) => (
          <FeatureSection key={f.number} number={f.number} title={f.title} description={f.description} />
        ))}
      </div>
      <HowItWorks />
      <div className="section-gap">
        <InstallBlock
          title="Install zombiectl, then run /usezombie-install-platform-ops"
          command="npm install -g @usezombie/zombiectl"
          actions={[
            { label: "Read the docs", to: DOCS_URL, variant: "ghost" },
            { label: "Install platform-ops", to: DOCS_QUICKSTART_URL, variant: "double-border" },
          ]}
        />
      </div>
      <div className="cta-row" style={{ marginTop: "1rem" }}>
        <Link className="cta ghost" to="/pricing">
          View full pricing
        </Link>
      </div>
    </section>
  );
}
