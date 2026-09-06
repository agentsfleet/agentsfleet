/**
 * Paint-boundary blank-frame audit for the dashboard shell.
 *
 * Both functions run INSIDE the browser via page.evaluate(), so each must be
 * fully self-contained (no captured module scope). They are exported as
 * plain functions so the unit lane can drive the same logic against a DOM
 * (tests/blank-frame-audit.test.ts — vitest excludes tests/e2e/** as test
 * files, so the sibling unit test lives one level up).
 *
 * Why requestAnimationFrame and not a MutationObserver: observer callbacks
 * run at mutation microtasks, BETWEEN React commits — states the compositor
 * never paints. Counting those reports "blank frames" no user can see, which
 * made the shell journeys intermittently fail on back-to-back navigations.
 * rAF fires immediately before the next paint; what it observes is what
 * ships to the screen, so a genuinely blanked `main` still counts.
 */

export interface ShellBlankAudit {
  blankFrames: number;
  main: HTMLElement;
  stopped: boolean;
  samples: Array<{
    at: number;
    pathname: string;
    connected: boolean;
    sameMain: boolean;
    textLength: number;
    childTags: string[];
  }>;
}

type AuditedWindow = typeof window & { __shellBlankAudit?: ShellBlankAudit };

export function installPaintBoundaryAudit(): void {
  const main = document.querySelector("main");
  if (!main) throw new Error("dashboard main region is missing");
  // This function is serialized into the page, so limits live inside it.
  const MAX_SAMPLES = 8;
  const MAX_CHILD_TAGS = 8;
  const audit: ShellBlankAudit = { blankFrames: 0, main, stopped: false, samples: [] };
  const inspect = (): void => {
    if (audit.stopped) return;
    if (
      !main.isConnected ||
      document.querySelector("main") !== main ||
      !main.textContent?.trim()
    ) {
      audit.blankFrames += 1;
      if (audit.samples.length < MAX_SAMPLES) {
        // Record structure, never account text, markup, or URL query tokens.
        audit.samples.push({
          at: performance.now(),
          pathname: location.pathname,
          connected: main.isConnected,
          sameMain: document.querySelector("main") === main,
          textLength: main.textContent?.trim().length ?? 0,
          childTags: Array.from(main.children).slice(0, MAX_CHILD_TAGS).map(child => child.tagName),
        });
      }
    }
    requestAnimationFrame(inspect);
  };
  requestAnimationFrame(inspect);
  (window as AuditedWindow).__shellBlankAudit = audit;
}

export function readBlankFrameEvidence(): Pick<ShellBlankAudit, "blankFrames" | "samples"> {
  const audit = (window as AuditedWindow).__shellBlankAudit;
  if (!audit) throw new Error("dashboard blank-frame audit is missing");
  return { blankFrames: audit.blankFrames, samples: audit.samples };
}

export function readBlankFrames(): number {
  const audit = (window as AuditedWindow).__shellBlankAudit;
  if (!audit) throw new Error("dashboard blank-frame audit is missing");
  audit.stopped = true;
  if (!audit.main.isConnected || document.querySelector("main") !== audit.main) {
    throw new Error("dashboard main region was replaced");
  }
  return audit.blankFrames;
}
