import { Link } from "react-router-dom";
import { Button, Terminal } from "@usezombie/design-system";
import { DOCS_QUICKSTART_URL } from "../config";
import { trackNavigationClicked, trackSignupStarted } from "../analytics/posthog";
export default function Hero() {
  const heading = {
    badge: "Always-on operational runtime · Markdown-defined",
    line1: "Operational knowledge isn't executable.",
    line2: "When a deploy fails, teams guess.",
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

          <p className="hero-kicker">
            Your deploy fails at 3am. Zombie wakes on the GitHub webhook, walks your
            CD logs + hosting + data-plane, posts the diagnosis to Slack with
            line-numbered evidence — every action recorded in a replayable event log.
            markdown is the only thing you write.
          </p>

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
              <br />
              <strong>$5 starter credit</strong> per workspace — no card required.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}
