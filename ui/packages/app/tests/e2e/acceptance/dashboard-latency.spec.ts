import { expect, test } from "@playwright/test";
import { AUDITED_PATH } from "@/lib/acceptance/workspace-fetch-audit";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { workspaceHref } from "./fixtures/nav";
import {
  LATENCY_SAMPLE_COUNT,
  attachStageTable,
  readAudit,
  resetAuditOrFail,
  soleDurationMs,
  summarizeStage,
  timed,
} from "./fixtures/latency";

/**
 * Where the dashboard's seconds go, measured rather than read off the source.
 *
 * Each surface is navigated a fixed number of times; the server-side fetch
 * audit answers what it asked for and the navigation is timed around it. The
 * tables land as run attachments, so every figure the architecture page quotes
 * names the run that produced it.
 *
 * This spec RESETS an app-global counter, so it runs in its own Playwright
 * project ordered after every other audited lane — see the `dashboard-latency`
 * project in `playwright.acceptance.config.ts`. Dropping it into the shared
 * `journeys` project would zero the counter under specs already counting.
 */

const LATENCY_SEED_PREFIX = "dash-latency-";
const CHAT_STAGE = "chat navigation (click → fleet sections visible)";
const SECRETS_STAGE = "secrets navigation (click → list visible)";
const EXPECTED_THREAD_READS_PER_LOAD = 1;
const EXPECTED_DETAIL_READS_PER_LOAD = 0;
const SECRETS_READS_PER_VISIT = 2;
const MAX_CONCURRENT_STREAMS = 1;
const SECRETS_LABEL = "Secrets";
const RUNNERS_LABEL = "Runners";
const RUNNERS_PATH = "/admin/runners";
const SECRETS_LIST_STAGE = "secrets list read (upstream)";
const SECRETS_PROVIDER_STAGE = "tenant provider read (upstream)";
const RUNNERS_STAGE = "runners navigation (click → wall visible)";
const RUNNERS_READ_STAGE = "runners list read (upstream)";

type StreamAudit = { maximum: number };

test.describe("dashboard load latency", () => {
  test.afterEach(async () => {
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, workspaceId, LATENCY_SEED_PREFIX);
  });

  test("test_chat_load_stages_are_measured_and_attached", async ({ page }, testInfo) => {
    // Counts the live connections a whole sample of navigations opens, so the
    // "one EventSource per workspace" claim is measured across route changes
    // rather than at a single instant.
    await page.addInitScript(() => {
      const NativeEventSource = window.EventSource;
      const audit = { active: 0, maximum: 0 };
      class AuditedEventSource extends NativeEventSource {
        private auditClosed = false;
        constructor(url: string | URL, options?: EventSourceInit) {
          super(url, options);
          audit.active += 1;
          audit.maximum = Math.max(audit.maximum, audit.active);
        }
        close() {
          if (!this.auditClosed) {
            this.auditClosed = true;
            audit.active -= 1;
          }
          super.close();
        }
      }
      window.EventSource = AuditedEventSource;
      (window as typeof window & { __streamAudit?: typeof audit }).__streamAudit = audit;
    });

    await signInAs(page, FIXTURE_KEY.regular);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, {
      name: `${LATENCY_SEED_PREFIX}${tag}`,
    });
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);

    const navigationMs: number[] = [];
    let threadReads = 0;
    let detailReads = 0;

    for (let sample = 0; sample < LATENCY_SAMPLE_COUNT; sample += 1) {
      await resetAuditOrFail(page);
      const { durationMs } = await timed(async () => {
        await page.goto(workspaceHref(workspaceId, `fleets/${fleet.id}`));
        await expect(page.getByRole("navigation", { name: "Fleet sections" })).toBeVisible();
      });
      navigationMs.push(durationMs);

      const snapshot = await readAudit(page);
      threadReads += snapshot.byPath[AUDITED_PATH.fleetMessages] ?? 0;
      detailReads += snapshot.byPath[AUDITED_PATH.fleetEventDetail] ?? 0;
    }

    expect(threadReads, "one thread read per chat load").toBe(
      EXPECTED_THREAD_READS_PER_LOAD * LATENCY_SAMPLE_COUNT,
    );
    expect(detailReads, "per-turn detail reads across the sample").toBe(
      EXPECTED_DETAIL_READS_PER_LOAD,
    );

    const streamAudit = await page.evaluate(
      () => (window as typeof window & { __streamAudit?: StreamAudit }).__streamAudit,
    );
    expect(
      streamAudit?.maximum ?? 0,
      "concurrent EventSource connections across the sample",
    ).toBeLessThanOrEqual(MAX_CONCURRENT_STREAMS);

    await attachStageTable(testInfo, "Chat load", [summarizeStage(CHAT_STAGE, navigationMs)]);
  });

  test("test_secrets_visit_pays_two_upstream_reads", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);

    await resetAuditOrFail(page);
    await page.goto(workspaceHref(workspaceId, "secrets"));
    await expect(page.getByRole("region", { name: SECRETS_LABEL })).toBeVisible();

    const first = await readAudit(page);
    expect(first.byPath[AUDITED_PATH.workspaceSecrets] ?? 0, "secrets list reads").toBe(1);
    expect(first.byPath[AUDITED_PATH.tenantProvider] ?? 0, "provider reads").toBe(1);
    expect(first.total, "upstream reads for one Secrets visit").toBe(SECRETS_READS_PER_VISIT);

    // Deliberately NOT reset: a second visit must add its own pair. The
    // per-render `cache()` around each read dedupes inside one render and has
    // never spanned navigations — this is what proves that, in a number.
    await page.goto(workspaceHref(workspaceId, "fleets"));
    await page.goto(workspaceHref(workspaceId, "secrets"));
    await expect(page.getByRole("region", { name: SECRETS_LABEL })).toBeVisible();

    const second = await readAudit(page);
    expect(second.total, "a second visit pays the pair again").toBe(
      SECRETS_READS_PER_VISIT * 2,
    );
  });

  test("test_secrets_stage_timings_are_measured_and_attached", async ({ page }, testInfo) => {
    await signInAs(page, FIXTURE_KEY.regular);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);

    const navigationMs: number[] = [];
    const secretsReadMs: number[] = [];
    const providerReadMs: number[] = [];

    for (let sample = 0; sample < LATENCY_SAMPLE_COUNT; sample += 1) {
      // Reset per sample so each window holds exactly one visit's reads; the
      // per-read durations are accumulated here rather than in the counter,
      // which the next reset would clear.
      await resetAuditOrFail(page);
      await page.goto(workspaceHref(workspaceId, "fleets"));
      const { durationMs } = await timed(async () => {
        await page.goto(workspaceHref(workspaceId, "secrets"));
        await expect(page.getByRole("region", { name: SECRETS_LABEL })).toBeVisible();
      });
      navigationMs.push(durationMs);

      const payload = await readAudit(page);
      expect(payload.total, "each sampled visit pays exactly its own pair").toBe(
        SECRETS_READS_PER_VISIT,
      );
      const secretsMs = soleDurationMs(payload, AUDITED_PATH.workspaceSecrets);
      const providerMs = soleDurationMs(payload, AUDITED_PATH.tenantProvider);
      expect(secretsMs, "the secrets list read was timed").not.toBeNull();
      expect(providerMs, "the provider read was timed").not.toBeNull();
      secretsReadMs.push(secretsMs ?? 0);
      providerReadMs.push(providerMs ?? 0);
    }

    const rows = [
      summarizeStage(SECRETS_STAGE, navigationMs),
      summarizeStage(SECRETS_LIST_STAGE, secretsReadMs),
      summarizeStage(SECRETS_PROVIDER_STAGE, providerReadMs),
    ];
    await attachStageTable(testInfo, "Secrets", rows);

    // The Section asks which of the two the wait belongs to; the answer is
    // attached beside the table so the page never has to re-derive it.
    const [, listRow, providerRow] = rows;
    const slower = (listRow?.p50Ms ?? 0) >= (providerRow?.p50Ms ?? 0) ? listRow : providerRow;
    await testInfo.attach("Secrets — slower read", {
      body: `The slower upstream read at p50 is **${slower?.stage}** ` +
        `(${slower?.p50Ms}ms p50, ${slower?.p95Ms}ms p95).\n`,
      contentType: "text/markdown",
    });
  });

  test("test_runners_navigation_returns_and_its_wait_is_attributed", async ({ page }, testInfo) => {
    // The operator fixture is the only one carrying `runner:read`; any other
    // lands on /settings via the page's scope guard and measures nothing.
    await signInAs(page, FIXTURE_KEY.operator);

    const navigationMs: number[] = [];
    const listReadMs: number[] = [];
    const attemptCounts: number[] = [];

    for (let sample = 0; sample < LATENCY_SAMPLE_COUNT; sample += 1) {
      await resetAuditOrFail(page);
      const { durationMs } = await timed(async () => {
        await page.goto(RUNNERS_PATH);
        // Returning at all is half the finding: it was reported as a page that
        // never comes back.
        await expect(page.getByRole("region", { name: RUNNERS_LABEL })).toBeVisible();
      });
      navigationMs.push(durationMs);

      const payload = await readAudit(page);
      const readMs = soleDurationMs(payload, AUDITED_PATH.fleetRunners);
      expect(readMs, "the runners list read was timed").not.toBeNull();
      listReadMs.push(readMs ?? 0);
      attemptCounts.push(payload.timingsByPath[AUDITED_PATH.fleetRunners]?.attempts[0] ?? 0);
    }

    await attachStageTable(testInfo, "Runners", [
      summarizeStage(RUNNERS_STAGE, navigationMs),
      summarizeStage(RUNNERS_READ_STAGE, listReadMs),
    ]);
    await testInfo.attach("Runners — attempts per navigation", {
      body: `Observed attempt counts: ${attemptCounts.join(", ")}.\n\n` +
        "One attempt per navigation means the retry ladder never climbed, so " +
        "the wait is the upstream read itself rather than a retry sleep.\n",
      contentType: "text/markdown",
    });
    // The scope check costs no upstream call — `hasScope` resolves from session
    // claims — so it is named here as eliminated rather than left open.
    expect(attemptCounts.length, "an attempt count per sampled navigation").toBe(
      LATENCY_SAMPLE_COUNT,
    );
  });
});
