import { useState } from "react";
import { Link } from "react-router-dom";
import {
  Button,
  LogLine,
  Terminal,
  WakePulse,
  useResettableTimeout,
} from "@usezombie/design-system";
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

// Bootstrap one-liner copied to the clipboard by the primary CTA. The
// CTA label is the visible form of the same command. Kept in lockstep
// with the OnboardingFlow `step 01` snippet so a user can copy from
// either surface and end up with identical text.
const HERO_INSTALL_COMMAND =
  "npm install -g @usezombie/zombiectl && npx skills add usezombie/usezombie";

const TOAST_VISIBLE_MS = 2000;

/*
 * Marketing hero — Mockup A canonical shape (DESIGN_SYSTEM.md preview.html).
 *
 *   eyebrow:  <WakePulse live> + LIVE label (mono, uppercase)
 *   headline: mono, two-line, "memorable thing" voice
 *   lede:     sans body, max 640px
 *   ctas:     terminal-style install button + ghost replay link
 *   cli:      inline <Terminal> showing the install transcript
 *
 * Primary CTA copies the bootstrap one-liner, surfaces an inline
 * aria-live toast, and smooth-scrolls down to the OnboardingFlow
 * anchor on the same page (#onboarding-flow). No portal — the toast
 * lives in the hero's own DOM so it ships under the same a11y tree.
 */
export default function Hero() {
  const [toast, setToast] = useState<null | "copied" | "manual">(null);
  const toastTimer = useResettableTimeout();

  function showToast(kind: "copied" | "manual") {
    setToast(kind);
    toastTimer.start(() => setToast(null), TOAST_VISIBLE_MS);
  }

  async function onInstallClick() {
    trackSignupStarted({ source: "hero_primary", surface: "hero", mode: "humans" });
    try {
      await navigator.clipboard.writeText(HERO_INSTALL_COMMAND);
      showToast("copied");
    } catch {
      showToast("manual");
    }
    const target = document.getElementById("onboarding-flow");
    if (target) {
      const prefersReducedMotion =
        typeof window.matchMedia === "function" &&
        window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      target.scrollIntoView({
        behavior: prefersReducedMotion ? "auto" : "smooth",
        block: "start",
      });
    }
  }

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
          className="font-mono text-fluid-hero leading-display-xl tracking-display-xl font-medium text-text"
          data-testid="hero-headline"
        >
          Your deploy failed.
          <br />
          The agent already knows why.
        </h1>

        <p className="font-sans text-body-lg leading-body-lg text-text-muted max-w-narrow">
          A long-lived runtime that owns one operational outcome end to end.
          Wakes on your events. Runs against a durable, replayable log. Posts
          evidenced answers — never chats.
        </p>

        <div className="flex flex-wrap gap-3 items-center">
          <Button
            type="button"
            onClick={() => void onInstallClick()}
            data-testid="hero-cta-primary"
            className="font-mono"
            aria-label="Copy the install command and scroll to onboarding"
          >
            <span className="text-pulse" aria-hidden="true">
              $
            </span>{" "}
            {HERO_INSTALL_COMMAND}
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
          <output
            aria-live="polite"
            data-testid="hero-cta-toast"
            className="font-mono text-mono text-text-muted"
          >
            {toast === "copied"
              ? "Copied — paste into your terminal"
              : toast === "manual"
                ? "Clipboard blocked — select the command above and copy manually"
                : null}
          </output>
        </div>

        <Terminal
          label="install platform-ops via Claude Code"
          data-testid="hero-cli"
          copyable
          copyText={HERO_INSTALL_TRANSCRIPT}
          className="max-w-wide"
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
