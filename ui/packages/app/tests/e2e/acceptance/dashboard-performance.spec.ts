import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import {
  getDefaultWorkspaceId,
  seedFleet,
} from "./fixtures/seed";
import {
  gotoWorkspace,
  workspaceUrlPattern,
} from "./fixtures/nav";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";
import {
  installPaintBoundaryAudit,
  readBlankFrames,
} from "./fixtures/blank-frame-audit";

const ROUTE_TIMEOUT_MS = 10_000;
const STREAM_PREFIX = `m143-fluidity-${crypto.randomBytes(4).toString("hex")}`;

async function installBlankFrameAudit(page: import("@playwright/test").Page) {
  await page.evaluate(installPaintBoundaryAudit);
}

async function blankFrameCount(
  page: import("@playwright/test").Page,
): Promise<number> {
  return page.evaluate(readBlankFrames);
}

test.describe("authenticated dashboard fluidity", () => {
  let streamWorkspaceId: string | null = null;

  test.afterEach(async () => {
    if (!streamWorkspaceId) return;
    await cleanWorkspaceFleets(
      FIXTURE_KEY.regular,
      streamWorkspaceId,
      STREAM_PREFIX,
    );
    streamWorkspaceId = null;
  });

  test(
    "test_shell_navigation_and_workspace_creation_survive_boundary_split",
    async ({ page }) => {
      await signInAs(page, FIXTURE_KEY.regular);
      await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets");
      await expect(page.locator('[data-glow="dashboard"]')).toBeVisible();
      await installBlankFrameAudit(page);

      const primaryNavigation = page.getByRole("navigation", {
        name: "Primary",
      });
      await primaryNavigation.getByRole("link", { name: "Events" }).click();
      await expect(page).toHaveURL(workspaceUrlPattern("events"), {
        timeout: ROUTE_TIMEOUT_MS,
      });
      await expect(page.getByRole("heading", { name: "Events" })).toBeVisible();

      const switcher = page.getByTestId("workspace-switcher");
      await switcher.click();
      await page.getByTestId("workspace-new").click();
      let createDialog = page.getByRole("dialog", {
        name: "Create workspace",
      });
      await expect(createDialog).toBeVisible();
      await createDialog.getByLabel("Name").fill("not-submitted");
      await createDialog.getByRole("button", { name: "Cancel" }).click();
      await expect(createDialog).toBeHidden();
      await expect(switcher).toBeFocused();

      await switcher.click();
      await page.getByTestId("workspace-new").click();
      createDialog = page.getByRole("dialog", { name: "Create workspace" });
      await expect(createDialog.getByLabel("Name")).toHaveValue("");
      await createDialog.getByRole("button", { name: "Cancel" }).click();

      await page.setViewportSize({ width: 390, height: 844 });
      const mobileTrigger = page.getByRole("button", {
        name: "Open navigation",
      });
      await mobileTrigger.click();
      const mobileNavigation = page.getByRole("dialog", {
        name: "Navigation",
      });
      await expect(mobileNavigation).toBeVisible();
      await mobileNavigation.getByRole("link", { name: "Approvals" }).click();
      await expect(page).toHaveURL(workspaceUrlPattern("approvals"), {
        timeout: ROUTE_TIMEOUT_MS,
      });
      await expect(mobileNavigation).toBeHidden();
      await expect(page.locator("main")).toBeVisible();
      expect(await blankFrameCount(page)).toBe(0);
    },
  );

  test(
    "test_inner_navigation_preserves_content_and_prefetch",
    async ({ page }) => {
      const prefetchedRoutes: string[] = [];
      page.on("request", (request) => {
        if (request.headers()["next-router-prefetch"] === "1") {
          prefetchedRoutes.push(request.url());
        }
      });

      await signInAs(page, FIXTURE_KEY.regular);
      await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets");
      await expect(page.locator('[data-glow="dashboard"]')).toBeVisible();
      await expect.poll(
        () =>
          prefetchedRoutes.some((url) => url.includes("/events")),
        { timeout: ROUTE_TIMEOUT_MS },
      ).toBe(true);
      await installBlankFrameAudit(page);

      const navigation = page.getByRole("navigation", { name: "Primary" });
      await navigation.getByRole("link", { name: "Approvals" }).click();
      await navigation.getByRole("link", { name: "Events" }).click();
      await expect(page).toHaveURL(workspaceUrlPattern("events"), {
        timeout: ROUTE_TIMEOUT_MS,
      });
      await expect(page.getByRole("heading", { name: "Events" })).toBeVisible();
      expect(await blankFrameCount(page)).toBe(0);
    },
  );

  test(
    "test_intent_loading_respects_client_capabilities",
    async ({ page }) => {
      await page.addInitScript(() => {
        Object.defineProperty(navigator, "connection", {
          configurable: true,
          value: { saveData: true },
        });
      });
      const scriptRequests: string[] = [];
      let applicationOrigin = "";
      page.on("request", (request) => {
        if (
          request.resourceType() === "script" &&
          new URL(request.url()).origin === applicationOrigin
        ) {
          scriptRequests.push(request.url());
        }
      });

      await signInAs(page, FIXTURE_KEY.operator);
      await page.goto("/admin/fleet-libraries");
      applicationOrigin = new URL(page.url()).origin;
      // Gate on content only the LOADED view carries — the route's loading
      // skeleton renders the same h1, and waiting for network silence is
      // unreachable here (the Clerk testing proxy holds retried FAPI
      // requests in-flight; the suite-hygiene test bans that wait).
      const trigger = page.getByRole("button", {
        name: "Create fleet library",
      });
      await expect(trigger).toBeVisible();
      scriptRequests.length = 0;

      await trigger.hover();
      await expect(trigger).toHaveAttribute(
        "data-intent-hover",
        "suppressed",
      );
      expect(scriptRequests).toHaveLength(0);

      await trigger.click();
      await expect(
        page.getByRole("dialog", { name: "Create fleet library" }),
      ).toBeVisible();
      expect(scriptRequests.length).toBeGreaterThan(0);
    },
  );

  test(
    "test_navigation_does_not_duplicate_live_subscriptions",
    async ({ page }) => {
      await page.addInitScript(() => {
        const NativeEventSource = window.EventSource;
        const audit = { active: 0, created: 0, maximum: 0 };
        class AuditedEventSource extends NativeEventSource {
          private auditClosed = false;

          constructor(url: string | URL, options?: EventSourceInit) {
            super(url, options);
            audit.active += 1;
            audit.created += 1;
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
        (window as typeof window & { __streamAudit?: typeof audit })
          .__streamAudit = audit;
      });

      streamWorkspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
      await seedFleet(FIXTURE_KEY.regular, streamWorkspaceId, {
        name: `${STREAM_PREFIX}-one`,
      });
      await signInAs(page, FIXTURE_KEY.regular);
      await page.goto(`/w/${streamWorkspaceId}/fleets`);
      await expect.poll(
        () =>
          page.evaluate(
            () =>
              (window as typeof window & {
                __streamAudit?: { created: number };
              }).__streamAudit?.created ?? 0,
          ),
        { timeout: ROUTE_TIMEOUT_MS },
      ).toBeGreaterThan(0);

      const navigation = page.getByRole("navigation", { name: "Primary" });
      await navigation.getByRole("link", { name: "Events" }).click();
      await expect(page).toHaveURL(workspaceUrlPattern("events"));
      await navigation.getByRole("link", { name: "Fleets" }).click();
      await expect(page).toHaveURL(workspaceUrlPattern("fleets"));

      const audit = await page.evaluate(
        () =>
          (window as typeof window & {
            __streamAudit?: {
              active: number;
              created: number;
              maximum: number;
            };
          }).__streamAudit,
      );
      expect(audit?.created).toBeGreaterThan(0);
      expect(audit?.active).toBeLessThanOrEqual(1);
      expect(audit?.maximum).toBeLessThanOrEqual(1);
      expect(
        await page.evaluate(
          () => performance.getEntriesByType("navigation").length,
        ),
      ).toBe(1);
    },
  );
});
