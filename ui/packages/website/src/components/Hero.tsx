import { Terminal } from "@usezombie/design-system";
import { APP_BASE_URL, DOCS_QUICKSTART_URL } from "../config";

type Props = {
  mode: "humans" | "agents";
};

export default function Hero({ mode }: Props) {
  const primaryCtaHref = mode === "humans"
    ? APP_BASE_URL
    : DOCS_QUICKSTART_URL;

  return (
    <section className="hero" aria-label="Hero">
      <div className="hero-inner">
        <div className="hero-copy">
          <p className="hero-badge">
            {mode === "humans"
              ? "For engineering teams \u00b7 BYOK \u00b7 No token markup"
              : "Agent delivery control plane \u00b7 OpenAPI \u00b7 CLI-first"}
          </p>

          <h1 className="hero-headline">
            <span className="hero-line1">Ship AI-generated PRs</span>
            <span className="hero-line2">without babysitting the run.</span>
          </h1>

          <p className="hero-kicker">
            UseZombie turns queued engineering work into validated pull requests with
            replay, policy controls, and isolated execution.
          </p>

          <div className="hero-cta-row">
            <a className="cta hero-cta-primary" href={primaryCtaHref}>
              Connect GitHub, automate PRs
            </a>
          </div>
        </div>

        <div className="hero-terminal-wrap">
          <Terminal label="Quick start command" copyable>
            {"curl -fsSL https://usezombie.sh/install.sh | bash"}
          </Terminal>
        </div>
      </div>
    </section>
  );
}
