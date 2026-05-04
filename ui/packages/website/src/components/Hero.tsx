import { Link } from "react-router-dom";
import { Button, Terminal } from "@usezombie/design-system";
import { DOCS_QUICKSTART_URL } from "../config";
import { trackNavigationClicked, trackSignupStarted } from "../analytics/posthog";
export default function Hero() {
  const heading = {
    badge: "Durable agent runtime · BYOK · Open source",
    line1: "Operational outcomes",
    line2: "don't fall into limbo.",
    kicker:
      "A durable, markdown-defined agent runtime. The platform-ops agent wakes on triggers — a deploy failure, a cron, a `zombiectl steer` — gathers evidence, and posts an evidenced diagnosis to Slack. BYOK. Open source.",
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
            <Button asChild className="hero-cta-primary">
              <a
                href={DOCS_QUICKSTART_URL}
                onClick={() => {
                  trackSignupStarted({ source: "hero_primary", surface: "hero", mode: "humans" });
                }}
              >
                Install platform-ops
              </a>
            </Button>
            <Button asChild variant="double-border" className="hero-cta-secondary">
              <Link
                to="/pricing"
                onClick={() => trackNavigationClicked({ source: "hero_secondary_pricing", surface: "hero", target: "pricing" })}
              >
                See pricing
              </Link>
            </Button>
          </div>

          <div className="hero-command-card">
            <p className="hero-command-label">Quick start command</p>
            <Terminal label="Quick start command" copyable>
              {"npm install -g @usezombie/zombiectl"}
            </Terminal>
            <p className="hero-command-note">
              Then run /usezombie-install-platform-ops in Claude Code, Amp, Codex CLI, or OpenCode.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}
