import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { expectAccessible, expectNoPageOverflow } from "./fixtures/accessibility";
import { EDIT_POLICY_LABEL } from "@/app/(dashboard)/admin/runners/components/RunnerListCells";
import { z } from "zod";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import { RUNNER_ADMIN_ACTION } from "@/lib/api/runners-types";

const RUNNERS = "/v1/fleets/runners";
const LABELS = ["acceptance", "header"];

for (const width of [1440, 390, 320]) {
  test.describe(`runner header at ${width}px`, () => {
    const client = () => clientFor(FIXTURE_KEY.policyEditor);
    let runnerId = "";
    const hasTouch = width < 768;
    const minimumTarget = hasTouch ? 44 : 32;
    test.use({ hasTouch });
    test.beforeAll(async () => {
      // A never-started labeled fixture cannot take work from the live runner.
      const runner = z.object({runner_id: z.uuid()}).parse(await client().post<unknown>("/v1/runners", {
        host_id: `header-layout-${crypto.randomUUID()}`, labels: LABELS,
        assigned_policy: {sandbox_tier: "landlock_full", network_policy: "allow_all", registry_allowlist: [], worker_count: 1, extra_binds: []},
      }));
      runnerId = runner.runner_id;
    });
    test.afterAll(async () => {
      if (!runnerId) return;
      await client().patch(`${RUNNERS}/${runnerId}`, {action: RUNNER_ADMIN_ACTION.revoke});
      await client().delete(`${RUNNERS}/${runnerId}`);
    });
    test(`read-only runner controls and metadata remain accessible at ${width}px`, async ({ page }) => {
      await page.setViewportSize({ width, height: 900 });
      await signInAs(page, "operator");
      await page.goto(`/admin/runners/${runnerId}`);
      const edit = page.getByRole("button", { name: EDIT_POLICY_LABEL, exact: true });
      const refresh = page.getByRole("button", { name: "Refresh", exact: true });
      await expect(edit).toHaveCount(0);
      await expect(refresh).toBeVisible();
      const refreshBox = await refresh.boundingBox();
      expect(refreshBox!.height).toBeGreaterThanOrEqual(minimumTarget);
      expect(refreshBox!.width).toBeGreaterThanOrEqual(minimumTarget);
      const labels = page.getByTestId("runner-labels").locator(":scope > *");
      await expect(labels).toContainText(LABELS);
      const sizes = await labels.evaluateAll(elements => elements.map(element => {
        const style = getComputedStyle(element);
        return { height: element.getBoundingClientRect().height, font: style.fontSize, line: style.lineHeight };
      }));
      expect(sizes.length).toBeGreaterThan(0);
      for (const size of sizes) expect(size).toEqual(sizes[0]);
      await expectNoPageOverflow(page);
      await expectAccessible(page, "Runner header and detail");
    });
  });
}
