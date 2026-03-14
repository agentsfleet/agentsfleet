import { Link } from "react-router-dom";
import Hero from "../components/Hero";
import FeatureSection from "../components/FeatureSection";
import FeatureFlow from "../components/FeatureFlow";
import HowItWorks from "../components/HowItWorks";
import { InstallBlock } from "@usezombie/design-system";
import { APP_BASE_URL, DOCS_URL } from "../config";

const features = [
  {
    number: "01",
    title: "Automated PR delivery",
    description:
      "Move from queued engineering intent to review-ready pull requests without manually orchestrating each run.",
  },
  {
    number: "02",
    title: "Resource-governed execution",
    description:
      "Upcoming paid plans add stricter sandbox memory, CPU, and disk controls so one runaway run cannot wreck the rest of the queue.",
  },
  {
    number: "03",
    title: "Built-in harness and validation",
    description:
      "Run checks before reviewers are pulled in. Validation output and run context ship with each PR so issues are found earlier.",
  },
  {
    number: "04",
    title: "Run quality that gets measured",
    description:
      "Score every run on completion, reliability, latency, and eventually resource efficiency so teams can see whether automation is actually improving.",
  },
  {
    number: "05",
    title: "Failure analysis with next-run context",
    description:
      "When a run fails, upcoming analysis surfaces why it failed and feeds the key lesson back into the next run instead of repeating the same mistake.",
  },
  {
    number: "06",
    title: "Dynamic agent profiles by repo and team",
    description:
      "Shape agent behavior per repo without rewriting workers, so each team can compile, activate, and audit the profile that matches its workflow.",
  },
  {
    number: "07",
    title: "Know exactly what the agent was allowed to do",
    description:
      "Require sign-off before agents touch sensitive code, enforce repo-specific rules automatically, and give reviewers a clear audit trail instead of guesswork.",
    },
];

export default function Home() {
  return (
    <section className="stack home-stack route-fade">
      <Hero />
      <FeatureFlow />
      <div className="section-gap home-section-head">
        <p className="eyebrow">What improves as you scale</p>
        <h2>Safer runs, clearer quality signals, and better automation over time.</h2>
      </div>
      <div className="grid two features-grid">
        {features.map((f) => (
          <FeatureSection key={f.number} number={f.number} title={f.title} description={f.description} />
        ))}
      </div>
      <HowItWorks />
      <div className="section-gap">
        <InstallBlock
          title="Install zombiectl and connect GitHub"
          command="curl -fsSL https://usezombie.sh/install.sh | bash"
          actions={[
            { label: "Read the docs", to: DOCS_URL, variant: "ghost" },
            { label: "Connect GitHub, automate PRs", to: APP_BASE_URL, variant: "double-border" },
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
