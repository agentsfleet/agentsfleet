import { expect, test, type Page } from "@playwright/test";
import { z } from "zod";
import { clientFor } from "./fixtures/api-client";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { workspaceHref } from "./fixtures/nav";

const MODEL_COUNT = 130;
const SERVER_PAGE_SIZE = 100;
const DISPLAY_PAGE_SIZE = 25;
const TEST_TIMEOUT_MS = 180_000;
const MODELS = "/v1/tenants/me/models";
const ENTRY = z.object({id: z.uuid()});
const POST = "POST";

// Own one credential and only the rows created by this invocation. No entry is
// activated, and no provider receives a request during this pagination test.
async function withModels(workspace: string, body: (prefix: string) => Promise<void>): Promise<void> {
  const client = clientFor(FIXTURE_KEY.regular);
  const prefix = `paging-${crypto.randomUUID()}`;
  const secrets = `/v1/workspaces/${workspace}/secrets`;
  const ids: string[] = [];
  const errors: unknown[] = [];
  await client.post(secrets, {name: prefix, data: {provider: "openai", api_key: crypto.randomUUID()}});
  try {
    for (let index = 0; index < MODEL_COUNT; index++) {
      const entry = ENTRY.parse(await client.post<unknown>(MODELS, {model_id: `${prefix}-${index}`, secret_ref: prefix}));
      ids.push(entry.id);
    }
    await body(prefix);
  } catch (error) {
    errors.push(error);
  } finally {
    const removed = await Promise.allSettled(ids.map((id) => client.delete(`${MODELS}/${id}`)));
    const credential = await Promise.allSettled([client.delete(`${secrets}/${prefix}`)]);
    const failures = [...removed, ...credential].filter((result) => result.status === "rejected");
    errors.push(...failures.map((result) => result.reason));
  }
  if (errors.length) throw new AggregateError(errors, "Pagination test or fixture cleanup failed");
}

async function holdNextPage(page: Page) {
  const ready = Promise.withResolvers<void>();
  const release = Promise.withResolvers<void>();
  const finished = Promise.withResolvers<void>();
  const pattern = "**/w/*/settings/models";
  let armed = true;
  await page.route(pattern, async (route) => {
    if (!armed || route.request().method() !== POST) return route.continue();
    armed = false;
    try {
      const response = await route.fetch();
      ready.resolve();
      await release.promise;
      // Forward the real server response unchanged; only its delivery is delayed.
      await route.fulfill({response});
    } catch (error) { ready.reject(error); throw error; }
    finally { finished.resolve(); }
  });
  return {ready: ready.promise, release: () => release.resolve(),
    async dispose() { release.resolve(); await page.unroute(pattern); if (!armed) await finished.promise; }};
}

test("changing Search during Next keeps Models on the new search's first page", async ({page}) => {
  test.setTimeout(TEST_TIMEOUT_MS);
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  await withModels(workspace, async (prefix) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(workspace, "settings/models"));
    const search = page.getByRole("searchbox", {name: "Search loaded models"});
    await search.fill(prefix);
    const next = page.getByRole("button", {name: "Next page"});
    const previous = page.getByRole("button", {name: "Previous page"});
    const table = page.getByRole("table", {name: "Models", exact: true});
    await expect(table.locator("tbody tr")).toHaveCount(DISPLAY_PAGE_SIZE);
    for (let index = 1; index < SERVER_PAGE_SIZE / DISPLAY_PAGE_SIZE; index++) await next.click();
    const held = await holdNextPage(page);
    try {
      await next.click();
      await held.ready;
      await search.fill(`${prefix}-`);
      await expect(previous).toBeDisabled();
      held.release();
      await expect(page.getByText(new RegExp(`^${MODEL_COUNT} of .* loaded models$`))).toBeVisible();
      await expect(previous).toBeDisabled();
      await expect(table.locator("tbody tr")).toHaveCount(DISPLAY_PAGE_SIZE);
    } finally { await held.dispose(); }
  });
});
