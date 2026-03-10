import { Link } from "react-router-dom";
import Hero from "../components/Hero";
import FeatureSection from "../components/FeatureSection";
import ProviderStrip from "../components/ProviderStrip";
import FeatureFlow from "../components/FeatureFlow";
import HowItWorks from "../components/HowItWorks";
import CTABlock from "../components/CTABlock";
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
    title: "Bring your own models",
    description:
      "Connect the model providers your team already uses. UseZombie focuses on controlled execution and delivery, not token resale.",
  },
  {
    number: "03",
    title: "Built-in harness and validation",
    description:
      "Run checks before reviewers are pulled in. Validation output and run context ship with each PR so issues are found earlier.",
  },
  {
    number: "04",
    title: "Custom agent profiles",
    description:
      "Define team-specific workflows for Echo, Scout, and Warden so each repo gets the right behavior and constraints.",
  },
  {
    number: "05",
    title: "Replay, observability, and isolation",
    description:
      "Investigate retries with full run replay, track agent output quality, and keep execution boundaries tighter for untrusted code.",
  },
  {
    number: "06",
    title: "Stop agents from piling up during a release",
    description:
      "Freeze runs before a deploy, prioritize the hotfix, and let everything else resume when you're ready — no more surprise PRs landing mid-release.",
  },
  {
    number: "07",
    title: "Know exactly what the agent was allowed to do",
    description:
      "Require sign-off before agents touch sensitive code, enforce repo-specific rules automatically, and give reviewers a clear audit trail instead of guesswork.",
  },
];

type Props = {
  mode: "humans" | "agents";
};

export default function Home({ mode }: Props) {
  if (mode === "humans") {
    return (
      <section className="stack home-stack route-fade">
        <Hero mode={mode} />
        <FeatureFlow />
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
      </section>
    );
  }

  return (
    <section className="stack home-stack route-fade">
      <Hero mode={mode} />
      <ProviderStrip />
      <div className="section-gap home-section-head">
        <p className="eyebrow">Features</p>
        <h2>What teams get from UseZombie</h2>
      </div>
      <div className="grid two features-grid">
        {features.map((f) => (
          <FeatureSection key={f.number} number={f.number} title={f.title} description={f.description} />
        ))}
      </div>
      <HowItWorks />
      <CTABlock />
      <div className="cta-row" style={{ marginTop: "1rem" }}>
        <Link className="cta ghost" to="/pricing">
          View full pricing
        </Link>
      </div>
    </section>
  );
}
