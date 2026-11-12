import { expect, test, type Page } from "@playwright/test";
import { z } from "zod";
import { clientFor } from "./fixtures/api-client";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { workspaceHref } from "./fixtures/nav";

const MODEL_COUNT = 130;
const SERVER_PAGE_SIZE = 100;
const TEST_TIMEOUT_MS = 180_000;
const MODELS = "/v1/tenants/me/models";
const ENTRY_ID = z.object({id: z.uuid()});
const ENTRY = ENTRY_ID.extend({workspace_id: z.uuid()});
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
      const entry = await client.post<unknown>(MODELS, {workspace_id: workspace, model_id: `${prefix}-${index}`, secret_ref: prefix});
      ids.push(ENTRY_ID.parse(entry).id);
      expect(ENTRY.parse(entry).workspace_id).toBe(workspace);
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

test("Load more retains the first page while delivery is delayed, then appends each model once", async ({page}) => {
  test.setTimeout(TEST_TIMEOUT_MS);
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  await withModels(workspace, async (prefix) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(workspace, "settings/models"));
    const models = page.getByRole("region", {name: "Your models", exact: true});
    const rows = models.getByRole("listitem");
    const more = page.getByRole("button", {name: "Load more models", exact: true});
    await expect(rows).toHaveCount(SERVER_PAGE_SIZE);
    const firstNames = await rows.evaluateAll(items => items.map(item => item.getAttribute("aria-label")));
    const held = await holdNextPage(page);
    try {
      await more.click();
      await held.ready;
      await expect(more).toBeDisabled();
      await expect(rows).toHaveCount(SERVER_PAGE_SIZE);
      expect(await rows.evaluateAll(items => items.map(item => item.getAttribute("aria-label")))).toEqual(firstNames);
      held.release();
      await expect(rows).not.toHaveCount(SERVER_PAGE_SIZE);
      while (await more.isVisible()) {
        const count = await rows.count();
        await more.click();
        await expect(rows).not.toHaveCount(count);
      }
      const names = await rows.evaluateAll(items => items.map(item => item.getAttribute("aria-label")));
      expect(names.filter(name => name?.startsWith(prefix))).toHaveLength(MODEL_COUNT);
      expect(new Set(names).size).toBe(names.length);
      for (const name of firstNames) expect(names).toContain(name);
    } finally { await held.dispose(); }
  });
});
