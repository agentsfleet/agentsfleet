import { useRef, useState } from "react";
import { Link } from "react-router-dom";
import {
  Button,
  LogLine,
  LogToken,
  Terminal,
  Toast,
  WakePulse,
  useResettableTimeout,
} from "@usezombie/design-system";
import { trackNavigationClicked, trackSignupStarted } from "../analytics/posthog";
import { INSTALL_COMMAND, INSTALL_SKILL_COMMAND } from "../config";
import { RATES_DISPLAY } from "../lib/rates";

const TOAST_VISIBLE_MS = 2000;

/*
 * Marketing hero — Mockup A canonical shape (DESIGN_SYSTEM.md preview.html).
 *
 *   eyebrow:  <WakePulse live> + LIVE label (mono, uppercase)
 *   headline: mono, two-line, "memorable thing" voice
 *   lede:     sans body, max 640px
 *   cta:      install command copy-row (curl one-liner) + ghost replay link
 *   cli:      animated <Terminal> showing the install running line-by-line
 *
 * The Copy affordance writes INSTALL_COMMAND to the clipboard and surfaces
 * an inline design-system <Toast> — copy only, no navigation (the old giant
 * button scrolled the page out from under the reader). The animated terminal
 * demos the install and copies the next-step slash command. The toast lives
 * in the hero's own DOM so it ships under the same a11y tree.
 */
export default function Hero() {
  const [toast, setToast] = useState<null | "copied" | "manual">(null);
  const toastTimer = useResettableTimeout();
  // Toast keeps its children mounted through the fade-out window so the
  // text fades rather than snapping to empty (design-system contract —
  // Toast.test.tsx "keeps children mounted during the fade window"). Hold
  // the last shown kind so children + severity stay stable while `visible`
  // is false and the fade plays; reading `toast` directly would reset both
  // the same paint it clears.
  const lastToast = useRef<"copied" | "manual">("copied");

  function showToast(kind: "copied" | "manual") {
    lastToast.current = kind;
    setToast(kind);
    toastTimer.start(() => setToast(null), TOAST_VISIBLE_MS);
  }

  async function onCopyInstall() {
    trackSignupStarted({ source: "hero_primary", surface: "hero", mode: "humans" });
    try {
      await navigator.clipboard.writeText(INSTALL_COMMAND);
      showToast("copied");
    } catch {
      showToast("manual");
    }
  }

  // During the fade-out (`toast === null`) fall back to the last shown
  // kind so the message + severity stay put while Toast plays its fade.
  const shown = toast ?? lastToast.current;

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

        <Link
          to="/pricing"
          onClick={() =>
            trackNavigationClicked({
              source: "hero_promo_pill",
              surface: "hero",
              target: "pricing",
            })
          }
          className="inline-flex w-fit items-center gap-2 rounded-full bg-card border border-border px-3 py-1 text-sm font-mono text-text hover:text-text transition-colors"
          data-testid="hero-promo-pill"
        >
          <span className="rounded-full bg-evidence text-background px-2 py-0.5 text-xs uppercase tracking-eyebrow font-medium">
            Promo
          </span>
          {RATES_DISPLAY.FREE_TRIAL_PILL}
          <span aria-hidden="true">→</span>
        </Link>

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

        <div className="flex flex-col gap-3 max-w-wide">
          {/* Install command — copy-only, no navigation. The long one-liner
            * gets an explicit Copy affordance instead of being a giant click
            * target that scrolled the page out from under the reader. */}
          <div className="flex items-center gap-3 rounded-md border border-border bg-surface-deep px-md py-sm">
            <span className="text-pulse" aria-hidden="true">
              $
            </span>
            <code
              className="flex-1 overflow-x-auto whitespace-nowrap font-mono text-mono text-text"
              data-testid="hero-install-command"
            >
              {INSTALL_COMMAND}
            </code>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={() => void onCopyInstall()}
              data-testid="hero-cta-primary"
              className="ml-auto shrink-0 h-auto py-0.5 font-mono text-label"
              aria-label="Copy the install command"
            >
              Copy
            </Button>
          </div>

          <div className="flex flex-wrap items-center gap-3">
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
                → view a real wake (replay)
              </Link>
            </Button>
            <Toast
              visible={toast !== null}
              severity={shown === "manual" ? "warning" : "info"}
              data-testid="hero-cta-toast"
            >
              {shown === "copied"
                ? "Copied — paste into your terminal"
                : "Clipboard blocked — select the command above and copy manually"}
            </Toast>
          </div>
        </div>

        <Terminal
          label="install via usezombie.sh"
          data-testid="hero-cli"
          copyable
          animate
          copyText={INSTALL_SKILL_COMMAND}
          className="max-w-wide"
        >
          {/* Animated, multi-colour demo of the one-command install
            * (curl → zombiectl + skill bundle), ending in the next-step
            * slash command. Per-token colour via <LogToken> (prompt, host,
            * binaries, identifiers, next command) so it reads live, not
            * monotone. Copy hands back exactly that slash command. */}
          <LogLine severity="info"><LogToken severity="pulse">$</LogToken> {INSTALL_COMMAND}</LogLine>
          <LogLine severity="debug">› detecting host… <LogToken severity="info">claude code</LogToken></LogLine>
          <LogLine severity="debug">› installing <LogToken severity="info">zombiectl</LogToken> + skill bundle…</LogLine>
          <LogLine severity="done">✓ zombiectl on PATH · skill added (<LogToken severity="evidence">usezombie/usezombie</LogToken>)</LogLine>
          <LogLine severity="done">✓ webhook registered <LogToken severity="evidence">github.com/your-org/your-repo</LogToken></LogLine>
          <LogLine severity="debug">→ next: <LogToken severity="pulse">{INSTALL_SKILL_COMMAND}</LogToken></LogLine>
        </Terminal>
      </div>
    </section>
  );
}
