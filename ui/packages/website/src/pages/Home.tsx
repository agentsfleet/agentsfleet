import { Link } from "react-router-dom";
import Hero from "../components/Hero";
import FeatureSection from "../components/FeatureSection";
import FeatureFlow from "../components/FeatureFlow";
import HowItWorks from "../components/HowItWorks";
import CTABlock from "../components/CTABlock";
import FAQ from "../components/FAQ";
import { Button, InstallBlock } from "@usezombie/design-system";
import { DOCS_QUICKSTART_URL, DOCS_URL } from "../config";

const features = [
  {
    number: "01",
    title: "Markdown-defined",
    description: "SKILL.md + TRIGGER.md. Iterate on prose, not redeploys.",
  },
  {
    number: "02",
    title: "BYOK",
    description: "Bring your own LLM key. We never mark up inference.",
  },
  {
    number: "03",
    title: "Approval gating",
    description:
      "Risky actions block until a human clicks Approve. State survives worker restarts.",
  },
  {
    number: "04",
    title: "Open source",
    description:
      "The code that holds your credentials and runs against your infra is code you can read.",
  },
];

export default function Home() {
  return (
    <div data-testid="home-page">
      <Hero />
      <FeatureFlow />

      <section className="site-section" aria-label="Core capabilities">
        <div className="wrap flex flex-col gap-8">
          <div className="flex flex-col gap-3">
            <p className="font-mono text-[11px] uppercase tracking-[0.1em] text-text-muted m-0">
              core capabilities
            </p>
            <h2 className="font-mono text-[clamp(28px,4vw,40px)] leading-[1.15] tracking-[-0.02em] font-medium text-text m-0 max-w-[760px]">
              A long-lived runtime that owns the outcome until it&apos;s resolved or blocked.
            </h2>
          </div>
          <div className="grid gap-4 grid-cols-1 sm:grid-cols-2 lg:grid-cols-4">
            {features.map((f) => (
              <FeatureSection
                key={f.number}
                number={f.number}
                title={f.title}
                description={f.description}
              />
            ))}
          </div>
        </div>
      </section>

      <HowItWorks />

      <section className="site-section">
        <div className="wrap">
          <InstallBlock
            title="Install zombiectl, then run /usezombie-install-platform-ops"
            command="npm install -g @usezombie/zombiectl"
            actions={[
              { label: "read the docs", to: DOCS_URL, variant: "ghost" },
              { label: "→ start an agent", to: DOCS_QUICKSTART_URL, variant: "default" },
            ]}
          />
          <div className="mt-6">
            <Button asChild variant="ghost">
              <Link to="/pricing">view full pricing</Link>
            </Button>
          </div>
        </div>
      </section>

      <FAQ />
      <CTABlock />
    </div>
  );
}
