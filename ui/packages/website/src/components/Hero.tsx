import { Link } from "react-router-dom";
import { Button, Terminal } from "@usezombie/design-system";
import { DOCS_QUICKSTART_URL } from "../config";
import { trackNavigationClicked, trackSignupStarted } from "../analytics/posthog";
export default function Hero() {
  const heading = {
    badge: "Always-on event-driven runtime · Markdown-defined",
    line1: "Agents that wake on every event.",
  };

  return (
    <section className="hero" aria-label="Hero">
      <div className="hero-inner hero-inner--humans">
        <div className="hero-copy">
          <p className="hero-badge">{heading.badge}</p>

          <h1 className="hero-headline">
            <span className="hero-line1">{heading.line1}</span>
          </h1>

          <p className="hero-kicker">
            A webhook wakes the Zombie Agent. It reads your logs, finds the
            cause, and posts the fix to Slack — every action on a replayable
            event log. You write only Markdown.
          </p>

          <div className="hero-cta-row">
            <Button asChild className="hero-cta-primary">
              <a
                href={DOCS_QUICKSTART_URL}
                onClick={() => {
                  trackSignupStarted({ source: "hero_primary", surface: "hero", mode: "humans" });
                }}
              >
                Start an Agent
              </a>
            </Button>
            <Button asChild variant="double-border" className="hero-cta-secondary">
              <Link
                to="/pricing"
                onClick={() => trackNavigationClicked({ source: "hero_secondary_pricing", surface: "hero", target: "pricing" })}
              >
                $5 starter credit &rarr;
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
              <strong>$5 starter credit</strong> &mdash; no card required.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}
