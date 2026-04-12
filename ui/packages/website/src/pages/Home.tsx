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
      "Turn queued engineering work into review-ready pull requests without manually shepherding each run.",
  },
  {
    number: "02",
    title: "Validation before review",
    description:
      "Run validation before reviewers are pulled in, and attach validation output to every pull request.",
  },
  {
    number: "03",
    title: "Replay and traceability",
    description:
      "Track each run from intent to PR with event history, replayable artifacts, and a clear audit trail.",
  },
  {
    number: "04",
    title: "Run quality scoring",
    description:
      "Score runs across completion, reliability, latency, and efficiency so teams can track whether automation is actually improving.",
  },
  {
    number: "05",
    title: "Failure analysis and improvement guidance",
    description:
      "When a run fails, surface the likely cause, preserve the right context, and guide the next run toward a better outcome.",
  },
  {
    number: "06",
    title: "Repo-level governance",
    description:
      "Apply repo and team-specific profiles, approvals, and sandbox limits so agents operate inside clear boundaries.",
  },
];

export default function Home() {
  return (
    <section className="stack home-stack route-fade">
      <Hero />
      <FeatureFlow />
      <div className="section-gap home-section-head">
        <p className="eyebrow">Core capabilities</p>
        <h2>Validated PR delivery, measurable run quality, and tighter control as automation scales.</h2>
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
