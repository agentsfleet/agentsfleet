import { expect } from "@playwright/test";
import { test } from "./fixtures/runner-configuration";

for (const theme of ["dark", "light"]) {
  test(`edits runner configuration with a reachable save footer (${theme})`, async ({page, runnerConfiguration}, info) => {
    const writes: unknown[] = [];
    const errors: string[] = [];
    page.on("pageerror", error => errors.push(error.message));
    await page.route("**/runner/preview-runner", async route => {
      writes.push(route.request().postDataJSON());
      await route.fulfill({json: {ok: true, data: {id: "preview-runner"}}});
    });
    await page.goto(runnerConfiguration);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    await page.getByRole("button", {name: "Edit", exact: true}).click();
    const dialog = page.getByRole("dialog", {name: "Edit runner"});
    const save = dialog.getByRole("button", {name: "Save", exact: true});
    await expect(dialog).toBeVisible();
    await expect(dialog).toHaveCSS("opacity", "1");
    await expect(save).toBeInViewport();
    const initial = info.outputPath(`runner-editor-${theme}.png`);
    await page.screenshot({path: initial});
    await info.attach(`runner-editor-${theme}`, {path: initial, contentType: "image/png"});
    await dialog.getByLabel("Workers", {exact: true}).fill("12");
    await dialog.getByRole("switch", {name: "Enable all public internet access"}).click();
    await dialog.getByRole("switch", {name: "Durable tasks"}).click();
    await dialog.getByLabel("Labels", {exact: true}).fill("linux, eu-west, ci");
    await expect(save).toBeInViewport();
    const footer = await save.boundingBox();
    const panel = await dialog.boundingBox();
    if (!footer || !panel) throw new Error("Editor and save geometry must be available");
    expect(footer.y + footer.height).toBeLessThanOrEqual(page.viewportSize()?.height ?? 0);
    expect(panel.x + panel.width).toBe(page.viewportSize()?.width);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await save.click();
    await expect(dialog).toHaveCount(0);
    await expect(page.getByRole("status")).toHaveText("Runner configuration saved.");
    expect(writes).toEqual([{assigned_policy: {
      decision_model_id: "0199f0c0-0000-7000-8000-000000000003",
      durable_tasks_enabled: false, sandbox_tier: "landlock_full", network_policy: "allow_list_egress",
      registry_allowlist: ["registry.npmjs.org", "pypi.org"], worker_count: 12,
      extra_binds: [{path: "/srv/models", mode: "read_only", note: "Shared model weights"}],
    }, labels: ["linux", "eu-west", "ci"]}]);
    await expect(page.getByRole("button", {name: "Edit", exact: true})).toBeFocused();
    expect(errors).toEqual([]);
  });

  test(`keeps runner edits after a failed save (${theme})`, async ({page, runnerConfiguration}) => {
    let requests = 0;
    await page.route("**/runner/preview-runner", async route => {
      requests += 1;
      await route.fulfill({json: requests === 1
        ? {ok: false, error: "Runner not found", errorCode: "UZ-RUN-014"}
        : {ok: true, data: {id: "preview-runner"}}});
    });
    await page.goto(runnerConfiguration);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    await page.getByRole("button", {name: "Edit", exact: true}).click();
    await page.getByLabel("Workers", {exact: true}).fill("8");
    await page.getByRole("button", {name: "Save", exact: true}).click();
    await expect(page.getByRole("alert")).toContainText("Runner not found");
    await expect(page.getByLabel("Workers", {exact: true})).toHaveValue("8");
    await expect(page.getByRole("button", {name: "Save", exact: true})).toBeInViewport();
    await page.getByRole("button", {name: "Save", exact: true}).click();
    await expect(page.getByRole("dialog")).toHaveCount(0);
    expect(requests).toBe(2);
  });
}
