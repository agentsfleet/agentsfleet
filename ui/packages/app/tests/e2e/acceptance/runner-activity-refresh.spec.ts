import { expect, test } from "@playwright/test";
import { z } from "zod";
import { RUNNER_ADMIN_ACTION } from "@/lib/api/runners-types";
import { runnerPath, RUNNER_VIEW } from "@/lib/runner-routes";
import { ACTIVITY_TABLE_LABEL, REFRESH_RUNNER_LABEL } from "@/app/(dashboard)/admin/runners/[runnerId]/components/runner-copy";
import { clientFor } from "./fixtures/api-client";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";

const RUNNERS = "/v1/fleets/runners";
const CORDONED = "New work paused";
const REGISTERED = "Runner registered";

test("Refresh replaces the mounted Runner activity with the new lifecycle record", async ({page}) => {
  // A fresh, never-started host cannot take work from the acceptance runner.
  const client = clientFor(FIXTURE_KEY.policyEditor);
  const runner = z.object({runner_id: z.uuid()}).parse(await client.post<unknown>("/v1/runners", {
    host_id: `refresh-${crypto.randomUUID()}`, labels: [],
    assigned_policy: {sandbox_tier: "landlock_full", network_policy: "allow_all", registry_allowlist: [], worker_count: 1, extra_binds: []},
  }));
  const path = `${RUNNERS}/${runner.runner_id}`;
  try {
    await signInAs(page, FIXTURE_KEY.policyEditor);
    await page.goto(runnerPath(runner.runner_id, RUNNER_VIEW.activity));
    const table = page.getByRole("table", {name: ACTIVITY_TABLE_LABEL});
    await expect(table.getByRole("cell", {name: REGISTERED, exact: true})).toBeVisible();
    await expect(table.getByRole("cell", {name: CORDONED, exact: true})).toHaveCount(0);
    await client.patch(path, {action: RUNNER_ADMIN_ACTION.cordon});
    await page.getByRole("button", {name: REFRESH_RUNNER_LABEL, exact: true}).click();
    await expect(table.getByRole("cell", {name: CORDONED, exact: true})).toBeVisible();
    await expect(table.getByRole("cell", {name: REGISTERED, exact: true})).toBeVisible();
  } finally {
    await client.patch(path, {action: RUNNER_ADMIN_ACTION.revoke});
    await client.delete(path);
  }
});
