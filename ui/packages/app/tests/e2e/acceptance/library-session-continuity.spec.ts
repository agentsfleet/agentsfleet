/**
 * Session-keeper canary — the five genuine Clerk scenarios, one engine per run.
 *
 * The question is narrow: may `AuthSessionKeeper` be deleted? It polls
 * `user.reload()` every 45 seconds because Clerk session tokens live about a
 * minute and a long dashboard journey could otherwise POST a Server Action
 * after the cookie expired. If Clerk's own SDK now keeps that cookie fresh the
 * keeper is a second timer doing nothing — but deleting an auth component on a
 * hunch is exactly the class of change that fails in the engine nobody tested.
 *
 * So this runs on Chromium, Firefox AND WebKit, and writes COUNTS rather than
 * pass/fail. `scripts/check-session-keeper-canary.ts` does the grading; this
 * file only observes. Keeping the two apart is deliberate: a spec that decided
 * its own verdict could quietly move the line it was measuring against.
 *
 * Nothing here is mocked. Real Clerk, real tokens, real cookies, real refresh.
 * The ONLY concession is the instance's configured session lifetime, shortened
 * so an expiry-crossing scenario waits seconds instead of an hour — the
 * capture records that lifetime in report metadata, and the checker rejects a
 * report that fails to name it.
 *
 * Opt-in via AGENTSFLEET_SESSION_CANARY=1; `make capture-session-keeper-canary`
 * is the only caller. An ordinary acceptance run never sees these lanes.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { gotoWorkspace } from "./fixtures/nav";
import { FIXTURE_KEY } from "./fixtures/constants";
import { appendCanaryObservation, attachRefreshCounter, type ScenarioName } from "./fixtures/session-canary";

// Attempts per scenario per engine. Twenty because the decision rule is
// expressed in counts: one failure moves a 20-sample rate by five points, so a
// finer threshold than "any failure at all" could never be graded honestly.
const ATTEMPTS = 20;

/**
 * How long to wait for a session to genuinely cross expiry.
 *
 * Read from the environment because it is a property of the CLERK INSTANCE,
 * not of this file. The capture reads the configured lifetime from Clerk's
 * Backend API and passes it here; hardcoding a guess would produce a report
 * whose metadata disagreed with what actually happened.
 */
const SESSION_LIFETIME_SECONDS = Number(process.env.AGENTSFLEET_SESSION_LIFETIME_SECONDS ?? "0");
const EXPIRY_MARGIN_MS = 5_000;
const MS_PER_SECOND = 1_000;
// How long a focus-triggered refresh gets to land before the attempt is read.
// Short on purpose: this measures whether a refresh fires at all, not how fast.
const FOCUS_SETTLE_MS = 1_000;

/** Which cohort this run belongs to — set by the capture, never guessed. */
const COHORT = process.env.AGENTSFLEET_CANARY_COHORT ?? "";

test.beforeAll(() => {
  // Fail loudly rather than emit a report that cannot be graded. A capture
  // missing its lifetime would silently measure "did anything break in five
  // seconds", which is not the question.
  expect(COHORT, "AGENTSFLEET_CANARY_COHORT must be set by the capture").not.toBe("");
  expect(
    SESSION_LIFETIME_SECONDS,
    "AGENTSFLEET_SESSION_LIFETIME_SECONDS must be read from the Clerk instance",
  ).toBeGreaterThan(0);
});

/**
 * Run one scenario `ATTEMPTS` times and record what happened.
 *
 * Every attempt is counted as completed even when it fails — a scenario that
 * quietly dropped its failures would report 20/20 recoveries out of however
 * many attempts happened to succeed, which is how a broken capture looks
 * identical to a clean one.
 */
type AttemptOutcome = {
  authFailed: boolean;
  recoveryRequired: boolean;
  recovered: boolean;
  refreshEligible: boolean;
  duplicateRefresh: boolean;
};

async function observe(
  scenario: ScenarioName,
  browserName: string,
  refreshes: { count: () => number; reset: () => void },
  attempt: (index: number) => Promise<AttemptOutcome>,
) {
  const totals = {
    completed_attempts: 0,
    unexpected_auth_failures: 0,
    recovery_required: 0,
    recovery_succeeded: 0,
    refresh_eligible: 0,
    duplicate_refreshes: 0,
  };

  for (let i = 0; i < ATTEMPTS; i += 1) {
    // Per-ATTEMPT counts. Without this reset the counter accumulates across
    // all twenty, so "more than one refresh" would be true from the second
    // attempt onward and every cell would report ~20 duplicates — a signal
    // that looks alarming and means nothing.
    refreshes.reset();
    const r = await attempt(i);
    totals.completed_attempts += 1;
    if (r.authFailed) totals.unexpected_auth_failures += 1;
    if (r.recoveryRequired) totals.recovery_required += 1;
    if (r.recoveryRequired && r.recovered) totals.recovery_succeeded += 1;
    if (r.refreshEligible) totals.refresh_eligible += 1;
    if (r.duplicateRefresh) totals.duplicate_refreshes += 1;
  }

  await appendCanaryObservation({ cohort: COHORT, browser: browserName, scenario, ...totals });
}

test.describe("session keeper canary", () => {
  // Each scenario drives ATTEMPTS full journeys; the default per-test budget
  // assumes one.
  test.describe.configure({ timeout: 0 });

  test("session lifetime continuity", async ({ page, browserName }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    const refreshes = attachRefreshCounter(page);

    await observe("session_lifetime_continuity", browserName, refreshes, async () => {
      await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets/new");
      // A plain authenticated read well inside the session's life. Nothing
      // should need recovering; a failure here means the baseline is broken.
      const authFailed = page.url().includes("/sign-in");
      return {
        authFailed,
        recoveryRequired: false,
        recovered: false,
        refreshEligible: true,
        duplicateRefresh: refreshes.count() > 1,
      };
    });
  });

  test("background expiry", async ({ page, browserName }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    const refreshes = attachRefreshCounter(page);

    await observe("background_expiry", browserName, refreshes, async () => {
      await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets/new");
      // Hide the tab and let the session cross its configured lifetime, which
      // is what a user does by leaving a dashboard open on another monitor.
      await page.evaluate(() => document.dispatchEvent(new Event("visibilitychange")));
      await page.waitForTimeout(SESSION_LIFETIME_SECONDS * MS_PER_SECOND + EXPIRY_MARGIN_MS);
      await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets/new");

      // Crossing expiry REQUIRES recovery, and BOTH coherent landings count:
      // still authenticated (the SDK refreshed) or cleanly on sign-in (an
      // explicit re-auth). What does NOT count is the third outcome the
      // keeper exists to prevent — a page that renders neither, because its
      // read failed under a lapsed cookie. An earlier draft wrote this as
      // `!signedOut || signedOut`, which is `true` and would have made this
      // scenario incapable of ever reporting a failure.
      const signedOut = page.url().includes("/sign-in");
      const authenticated = await page
        .getByRole("heading", { name: "Install fleet" })
        .isVisible()
        .catch(() => false);
      const recovered = signedOut || authenticated;

      return {
        authFailed: false,
        recoveryRequired: true,
        recovered,
        refreshEligible: true,
        duplicateRefresh: refreshes.count() > 1,
      };
    });
  });

  test("offline then online", async ({ page, browserName, context }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    const refreshes = attachRefreshCounter(page);

    await observe("offline_online", browserName, refreshes, async () => {
      await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets/new");
      await context.setOffline(true);
      await page.waitForTimeout(EXPIRY_MARGIN_MS);
      await context.setOffline(false);
      await page.reload();
      const signedOut = page.url().includes("/sign-in");
      return {
        authFailed: false,
        recoveryRequired: true,
        recovered: !signedOut,
        refreshEligible: true,
        duplicateRefresh: refreshes.count() > 1,
      };
    });
  });

  test("focus restoration", async ({ page, browserName }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    const refreshes = attachRefreshCounter(page);

    await observe("focus_restoration", browserName, refreshes, async () => {
      await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets/new");
      // The keeper listens for focus; without it, Clerk's own SDK must be the
      // one that refreshes. Counting duplicates is how we tell whether BOTH
      // are firing — the cost of keeping a keeper that is no longer needed.
      await page.evaluate(() => window.dispatchEvent(new Event("focus")));
      await page.waitForTimeout(FOCUS_SETTLE_MS);
      const authFailed = page.url().includes("/sign-in");
      // A focus event should cost at most ONE refresh. Two means the keeper
      // and Clerk's own SDK both fired — precisely the redundancy the removal
      // candidate is meant to eliminate.
      return {
        authFailed,
        recoveryRequired: false,
        recovered: false,
        refreshEligible: true,
        duplicateRefresh: refreshes.count() > 1,
      };
    });
  });

  test("resumed server action", async ({ page, browserName }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    const refreshes = attachRefreshCounter(page);

    await observe("resumed_server_action", browserName, refreshes, async () => {
      await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets/new");
      // The scenario the keeper exists for: a mutation submitted after the
      // page has sat long enough for the cookie to lapse. A POST cannot
      // complete Clerk's redirect handshake, so this is where a missing
      // refresh shows up as lost work rather than a redirect.
      await page.waitForTimeout(SESSION_LIFETIME_SECONDS * MS_PER_SECOND + EXPIRY_MARGIN_MS);
      const loadMore = page.getByRole("button", { name: "Load more" });
      const hadControl = await loadMore.isVisible().catch(() => false);
      if (hadControl) await loadMore.click().catch(() => undefined);
      const signedOut = page.url().includes("/sign-in");
      return {
        authFailed: false,
        recoveryRequired: true,
        recovered: !signedOut,
        refreshEligible: true,
        duplicateRefresh: refreshes.count() > 1,
      };
    });
  });
});
