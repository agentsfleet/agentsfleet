import { useState } from "react";
import { AgentIllustration } from "./AgentIllustration";
import { Button, DisplayXL, Toast, Section, useResettableTimeout } from "@agentsfleet/design-system";
import { trackNavigationClicked, trackSignupStarted } from "../analytics/posthog";
import { GITHUB_URL, INSTALL_COMMAND, WAITLIST_URL } from "../config";
import { HERO_HEADLINE, HERO_LEDE_PARTS, HERO_PRIMARY_LABEL, HERO_SECONDARY_LABEL, HOW_IT_WORKS_ANCHOR_ID } from "../lib/marketing-copy";

type CopyStatus = "copied" | "manual";
const TOAST_VISIBLE_MS = 2000;
const COPY_STATUS = { copied: "copied", manual: "manual" } as const;

export default function Hero() {
  const [toastVisible, setToastVisible] = useState(false);
  const [shown, setShown] = useState<CopyStatus>(COPY_STATUS.copied);
  const toastTimer = useResettableTimeout();
  function showToast(kind: CopyStatus) {
    setShown(kind);
    setToastVisible(true);
    toastTimer.start(() => setToastVisible(false), TOAST_VISIBLE_MS);
  }
  async function onCopyInstall() {
    try {
      await navigator.clipboard.writeText(INSTALL_COMMAND);
      showToast(COPY_STATUS.copied);
    } catch {
      showToast(COPY_STATUS.manual);
    }
  }
  return (
    <Section asChild>
      <section className="site-section" aria-label="Hero" data-testid="hero">
        <div className="wrap hero-layout">
          <div className="hero-copy flex min-w-0 flex-col gap-6">
            <HeroHeading />
            <HeroActions />
            <InstallRow onCopy={() => void onCopyInstall()} />
            <Toast visible={toastVisible} severity={shown === COPY_STATUS.manual ? "warning" : "info"} data-testid="hero-cta-toast">
              {shown === COPY_STATUS.copied ? "Copied — paste into your terminal" : "Clipboard blocked — select the command above and copy manually"}
            </Toast>
          </div>
          <HeroExample />
        </div>
      </section>
    </Section>
  );
}

function HeroHeading() {
  return (
    <>
      <p className="inline-flex items-center gap-2 font-mono text-eyebrow uppercase tracking-eyebrow text-pulse" data-testid="hero-eyebrow">
        <span className="size-2 rounded-full bg-pulse" aria-hidden="true" />
        AI incident response for engineering teams
      </p>
      <a href="/#pricing"
        onClick={() => trackNavigationClicked({ source: "hero_promo_pill", surface: "hero", target: "pricing" })}
        className="inline-flex min-h-11 w-fit items-center gap-2 rounded-md bg-card border border-border px-3 py-1 text-body-sm font-sans text-text transition-colors hover:border-border-strong"
        data-testid="hero-promo-pill"
      >
        <span className="text-pulse">Early access</span>
        Help shape agentsfleet<span aria-hidden="true">→</span>
      </a>
      <DisplayXL data-testid="hero-headline" className="max-w-tagline">{HERO_HEADLINE}</DisplayXL>
      <p className="font-sans text-body-lg leading-body-lg text-text-muted max-w-narrow">
        {HERO_LEDE_PARTS.intro} <strong className="font-medium text-text">{HERO_LEDE_PARTS.teammates}</strong>{" "}
        {HERO_LEDE_PARTS.middle} <strong className="font-medium text-text">{HERO_LEDE_PARTS.recurringWork}</strong>{" "}
        {HERO_LEDE_PARTS.outro}
      </p>
    </>
  );
}

function InstallRow({ onCopy }: { onCopy: () => void }) {
  return (
    <div className="flex min-w-0 items-center gap-3 rounded-md border border-border bg-surface-deep px-md py-sm">
      <span className="text-pulse" aria-hidden="true">$</span>
      <code className="min-w-0 flex-1 break-all font-mono text-mono text-text" data-testid="hero-install-command">
        {INSTALL_COMMAND}
      </code>
      <Button type="button" variant="secondary" size="sm" onClick={onCopy}
        data-testid="hero-cta-primary" className="ml-auto min-h-11 shrink-0"
        aria-label="Copy the install command"
      >Copy</Button>
    </div>
  );
}

function HeroActions() {
  return (
    <div className="flex flex-wrap items-center gap-3">
      <Button wrap asChild className="min-h-11" data-testid="hero-cta-early-access">
        <a href={WAITLIST_URL} target="_blank" rel="noopener noreferrer"
          onClick={() => trackSignupStarted({ source: "hero_early_access", surface: "hero", mode: "humans" })}
        >→ {HERO_PRIMARY_LABEL}</a>
      </Button>
      <Button wrap asChild variant="ghost" className="min-h-11" data-testid="hero-cta-secondary">
        <a href={`/#${HOW_IT_WORKS_ANCHOR_ID}`}>{HERO_SECONDARY_LABEL}</a>
      </Button>
    </div>
  );
}

function HeroExample() {
  return (
    <figure className="m-0 flex min-w-0 flex-col items-center gap-4 text-center">
      <AgentIllustration className="w-48 max-w-full" />
      <figcaption className="max-w-trim font-sans text-body text-text-muted">
        Your logs, metrics, and code.<br />A diagnosis you can review.
      </figcaption>
      <p className="m-0 max-w-trim font-sans text-body-sm text-text-muted">
        <a href={GITHUB_URL} className="inline-flex min-h-11 items-center text-pulse underline" target="_blank" rel="noopener noreferrer">Open-source runtime</a>.
        Hosted today. Self-hosting is planned.
      </p>
    </figure>
  );
}
