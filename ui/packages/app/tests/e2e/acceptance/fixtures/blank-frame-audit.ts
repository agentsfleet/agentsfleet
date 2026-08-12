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
}

type AuditedWindow = typeof window & { __shellBlankAudit?: ShellBlankAudit };

export function installPaintBoundaryAudit(): void {
  const main = document.querySelector("main");
  if (!main) throw new Error("dashboard main region is missing");
  const audit = { blankFrames: 0, main, stopped: false };
  const inspect = (): void => {
    if (audit.stopped) return;
    if (
      !main.isConnected ||
      document.querySelector("main") !== main ||
      !main.textContent?.trim()
    ) {
      audit.blankFrames += 1;
    }
    requestAnimationFrame(inspect);
  };
  requestAnimationFrame(inspect);
  (window as AuditedWindow).__shellBlankAudit = audit;
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
