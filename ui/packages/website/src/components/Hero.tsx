import { Button, Terminal } from "@usezombie/design-system";
import { APP_BASE_URL } from "../config";
import { trackNavigationClicked, trackSignupStarted } from "../analytics/posthog";
export default function Hero() {
  const heading = {
    badge: "For engineering teams · BYOK · No token markup",
    line1: "Ship AI-generated PRs",
    line2: "without babysitting the run.",
    kicker:
      "UseZombie turns queued engineering work into validated pull requests with replay, run quality scoring, and policy controls so teams can improve automation with evidence, not guesswork.",
  };

  return (
    <section className="hero" aria-label="Hero">
      <div className="hero-inner hero-inner--humans">
        <div className="hero-copy">
          <p className="hero-badge">{heading.badge}</p>

          <h1 className="hero-headline">
            <span className="hero-line1">{heading.line1}</span>
            <span className="hero-line2">{heading.line2}</span>
          </h1>

          <p className="hero-kicker">{heading.kicker}</p>

          <div className="hero-cta-row">
            <Button
              to={APP_BASE_URL}
              className="hero-cta-primary"
              onClick={() => {
                trackSignupStarted({ source: "hero_primary", surface: "hero", mode: "humans" });
              }}
            >
              Connect GitHub, automate PRs
            </Button>
            <Button
              to="/pricing"
              variant="double-border"
              className="hero-cta-secondary"
              onClick={() => trackNavigationClicked({ source: "hero_secondary_pricing", surface: "hero", target: "pricing" })}
            >
              See pricing
            </Button>
          </div>

          <div className="hero-command-card">
            <p className="hero-command-label">Quick start command</p>
            <Terminal label="Quick start command" copyable>
              {"curl -fsSL https://usezombie.sh/install.sh | bash"}
            </Terminal>
            <p className="hero-command-note">
              Install `zombiectl`, connect GitHub, and keep the rest of your workflow intact.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}
