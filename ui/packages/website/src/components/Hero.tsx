import { Link } from "react-router-dom";
import { Button, LogLine, Terminal, WakePulse } from "@usezombie/design-system";
import { DOCS_QUICKSTART_URL } from "../config";
import { trackNavigationClicked, trackSignupStarted } from "../analytics/posthog";

// Plain-text payload for the clipboard. Visible terminal renders
// each line through <LogLine severity=...> so the prompt, debug
// breadcrumbs, and success markers each carry their token-driven
// colour per `preview.html` §03 — copy still hands the user a
// straight-from-the-terminal transcript with no escape codes.
const HERO_INSTALL_TRANSCRIPT = `$ claude /usezombie-install-platform-ops
› fetching SKILL.md from registry...
✓ installed platform-ops (SKILL.md · TRIGGER.md · 2 secrets injected via vault)
✓ webhook registered github.com/your-org/your-repo
› awaiting first event...`;

/*
 * Marketing hero — Mockup A canonical shape (DESIGN_SYSTEM.md preview.html).
 *
 *   eyebrow:  <WakePulse live> + LIVE label (mono, uppercase)
 *   headline: mono, two-line, "memorable thing" voice
 *   lede:     sans body, max 640px
 *   ctas:     primary install + default replay
 *   cli:      inline <Terminal> showing the install transcript
 *
 * No decorative hero illustration, proof grid, or animated gradient — the
 * dot-grid background (in styles.css) plus the eyebrow pulse carry all
 * visual life on this surface. --pulse currency rule honoured: it appears
 * exactly once on this page, on the live eyebrow indicator.
 */
export default function Hero() {
  return (
    <section className="site-section" aria-label="Hero" data-testid="hero">
      <div className="wrap flex flex-col gap-8">
        <p
          className="inline-flex items-center gap-2 font-mono text-eyebrow uppercase tracking-eyebrow text-pulse"
          data-testid="hero-eyebrow"
        >
          <WakePulse
            live
            aria-hidden="true"
            className="inline-block size-[6px] rounded-full bg-pulse"
          />
          LIVE — wake.on.event
        </p>

        <h1
          className="font-mono text-[clamp(2.5rem,7vw,4rem)] leading-[1.05] tracking-display-xl font-medium text-text"
          data-testid="hero-headline"
        >
          Your deploy failed.
          <br />
          The agent already knows why.
        </h1>

        <p className="font-sans text-body-lg leading-[1.5] text-text-muted max-w-[640px]">
          A long-lived runtime that owns one operational outcome end to end.
          Wakes on your events. Runs against a durable, replayable log. Posts
          evidenced answers — never chats.
        </p>

        <div className="flex flex-wrap gap-3 items-center">
          <Button asChild data-testid="hero-cta-primary">
            <a
              href={DOCS_QUICKSTART_URL}
              onClick={() =>
                trackSignupStarted({ source: "hero_primary", surface: "hero", mode: "humans" })
              }
            >
              → install in Claude Code
            </a>
          </Button>
          <Button asChild variant="ghost" data-testid="hero-cta-secondary">
            <Link
              to="/agents"
              onClick={() =>
                trackNavigationClicked({
                  source: "hero_secondary_replay",
                  surface: "hero",
                  target: "agents",
                })
              }
            >
              view a real wake (replay)
            </Link>
          </Button>
        </div>

        <Terminal
          label="install platform-ops via Claude Code"
          data-testid="hero-cli"
          copyable
          copyText={HERO_INSTALL_TRANSCRIPT}
          className="max-w-[860px]"
        >
          {/* Severity-coloured transcript per `preview.html` §03 logs
            * specimen. Prompt + breadcrumbs read as muted; success
            * markers (✓) carry the success token; the terminal stays
            * monochrome elsewhere. */}
          <LogLine severity="debug">$ claude /usezombie-install-platform-ops</LogLine>
          <LogLine severity="debug">› fetching SKILL.md from registry...</LogLine>
          <LogLine severity="done">✓ installed platform-ops (SKILL.md · TRIGGER.md · 2 secrets injected via vault)</LogLine>
          <LogLine severity="done">✓ webhook registered github.com/your-org/your-repo</LogLine>
          <LogLine severity="debug">› awaiting first event...</LogLine>
        </Terminal>
      </div>
    </section>
  );
}
